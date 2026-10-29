import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { setupWalletSelector, type WalletSelector, type WalletModuleFactory } from "@near-wallet-selector/core";
import { setupModal, type WalletSelectorModal } from "@near-wallet-selector/modal-ui";
import { setupMyNearWallet } from "@near-wallet-selector/my-near-wallet";
import { setupMeteorWallet } from "@near-wallet-selector/meteor-wallet";
import { actionCreators } from "@near-js/transactions";
import "@near-wallet-selector/modal-ui/styles.css";
import { config } from "./lib/config";
import { outcomeFailure, txHash, type CallSpec } from "./lib/calls";

export interface CallResult {
  ok: boolean;
  error?: string;
  hash?: string | null;
  outcome?: unknown;
}

interface WalletCtx {
  ready: boolean;
  accountId: string | null;
  error: string | null;
  signIn(): void;
  signOut(): Promise<void>;
  call(spec: CallSpec): Promise<CallResult>;
}

const Ctx = createContext<WalletCtx | null>(null);

export function useWallet(): WalletCtx {
  const c = useContext(Ctx);
  if (!c) throw new Error("WalletProvider missing");
  return c;
}

export function WalletProvider({ children }: { children: ReactNode }) {
  const selectorRef = useRef<WalletSelector | null>(null);
  const modalRef = useRef<WalletSelectorModal | null>(null);
  const [ready, setReady] = useState(false);
  const [accountId, setAccountId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (config.errors.length) {
      setReady(true);
      return;
    }
    let sub: { unsubscribe(): void } | undefined;
    let cancelled = false;
    (async () => {
      try {
        const modules: WalletModuleFactory[] = [setupMyNearWallet(), setupMeteorWallet()];
        // Staging-only test wallet; the constant check lets the bundler drop it from normal builds.
        if (import.meta.env.VITE_TEST_KEY_WALLET === "1") {
          const { setupTestWallet } = await import("./testWallet");
          modules.unshift(setupTestWallet({ rpc: config.rpc }) as WalletModuleFactory);
        }
        const selector = await setupWalletSelector({
          network: config.network,
          relayerUrl: config.relayer ?? undefined,
          modules,
        });
        if (cancelled) return;
        selectorRef.current = selector;
        modalRef.current = setupModal(selector, { contractId: config.core });
        sub = selector.store.observable.subscribe((s) => {
          setAccountId(s.accounts.find((a) => a.active)?.accountId ?? s.accounts[0]?.accountId ?? null);
        });
        setReady(true);
      } catch (e) {
        setError(`Wallet setup failed: ${e instanceof Error ? e.message : String(e)}`);
        setReady(true);
      }
    })();
    return () => {
      cancelled = true;
      sub?.unsubscribe();
    };
  }, []);

  const signIn = useCallback(() => {
    if (!modalRef.current) {
      setError("Wallet is not ready yet.");
      return;
    }
    modalRef.current.show();
  }, []);

  const signOut = useCallback(async () => {
    try {
      const w = await selectorRef.current?.wallet();
      await w?.signOut();
    } catch (e) {
      setError(`Sign out failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  }, []);

  const call = useCallback(async (spec: CallSpec): Promise<CallResult> => {
    try {
      const selector = selectorRef.current;
      if (!selector) return { ok: false, error: "Wallet is not ready." };
      const wallet = await selector.wallet();
      const outcome = await wallet.signAndSendTransaction({
        receiverId: spec.receiver,
        actions: [actionCreators.functionCall(spec.method, spec.args, spec.gas, spec.deposit)],
      });
      const failure = outcomeFailure(outcome);
      if (failure) return { ok: false, error: failure, outcome, hash: txHash(outcome) };
      return { ok: true, outcome, hash: txHash(outcome) };
    } catch (e) {
      return { ok: false, error: e instanceof Error ? e.message : String(e) };
    }
  }, []);

  const value = useMemo<WalletCtx>(
    () => ({ ready, accountId, error, signIn, signOut, call }),
    [ready, accountId, error, signIn, signOut, call],
  );
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}
