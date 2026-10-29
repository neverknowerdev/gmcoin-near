/**
 * STAGING ONLY. A wallet-selector module that signs with an ed25519 key read at
 * runtime from localStorage (gm_test_account / gm_test_key). It is registered
 * only when VITE_TEST_KEY_WALLET=1 and is not part of normal builds.
 */
import type { Account as SelAccount, InjectedWallet, WalletModuleFactory } from "@near-wallet-selector/core";
import { Account } from "@near-js/accounts";
import { KeyPairSigner } from "@near-js/signers";
import { JsonRpcProvider } from "@near-js/providers";
import { parseTestCreds, TEST_ACCOUNT_KEY, TEST_PRIVATE_KEY, type TestCreds } from "./lib/testCreds";
import { retryRateLimit } from "./lib/retry";

function readCreds(): TestCreds {
  return parseTestCreds(localStorage.getItem(TEST_ACCOUNT_KEY), localStorage.getItem(TEST_PRIVATE_KEY));
}

export function setupTestWallet({ rpc }: { rpc: string }): WalletModuleFactory<InjectedWallet> {
  return async () => ({
    id: "test-key-wallet",
    type: "injected",
    metadata: {
      name: "Test key wallet",
      description: "Staging only: signs with a key from localStorage.",
      iconUrl: "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24'%3E%3Ctext y='18' font-size='18'%3E%F0%9F%94%91%3C/text%3E%3C/svg%3E",
      deprecated: false,
      available: true,
      downloadUrl: "",
    },
    init: async () => {
      const provider = new JsonRpcProvider({ url: rpc }, { retries: 5, wait: 2000, backoff: 2 });
      const open = () => {
        const c = readCreds();
        const signer = KeyPairSigner.fromSecretKey(c.privateKey as `ed25519:${string}`);
        return { c, signer, account: new Account(c.accountId, provider, signer) };
      };
      const accounts = async (): Promise<SelAccount[]> => {
        try {
          const { c, signer } = open();
          return [{ accountId: c.accountId, publicKey: (await signer.getPublicKey()).toString() }];
        } catch {
          return [];
        }
      };
      return {
        async signIn() {
          const { c, signer } = open(); // throws a readable error if not configured
          return [{ accountId: c.accountId, publicKey: (await signer.getPublicKey()).toString() }];
        },
        async signOut() {},
        getAccounts: accounts,
        async verifyOwner() {
          throw new Error("verifyOwner is not supported by the test key wallet");
        },
        async signAndSendTransaction({ receiverId, actions }) {
          const { account } = open();
          if (!receiverId) throw new Error("receiverId is required");
          return retryRateLimit(() =>
            account.signAndSendTransaction({ receiverId, actions, waitUntil: "FINAL", throwOnFailure: false }),
          );
        },
        async signAndSendTransactions({ transactions }) {
          const { account } = open();
          const out = [];
          for (const t of transactions) {
            out.push(
              await retryRateLimit(() =>
                account.signAndSendTransaction({ receiverId: t.receiverId, actions: t.actions, waitUntil: "FINAL", throwOnFailure: false }),
              ),
            );
          }
          return out;
        },
        getPublicKey: async () => open().signer.getPublicKey(),
        signNep413Message: (...a: Parameters<KeyPairSigner["signNep413Message"]>) => open().signer.signNep413Message(...a),
        signTransaction: (...a: Parameters<KeyPairSigner["signTransaction"]>) => open().signer.signTransaction(...a),
        signDelegateAction: (...a: Parameters<KeyPairSigner["signDelegateAction"]>) => open().signer.signDelegateAction(...a),
      };
    },
  });
}
