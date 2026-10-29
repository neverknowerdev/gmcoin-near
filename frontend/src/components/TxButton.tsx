import { useState, type ReactNode } from "react";
import { useWallet } from "../wallet";
import { decodeSuccessValue, type CallSpec } from "../lib/calls";
import { config } from "../lib/config";

export function explorerTx(hash: string): string {
  const host = config.network === "mainnet" ? "nearblocks.io" : "testnet.nearblocks.io";
  return `https://${host}/txns/${hash}`;
}

interface Props {
  spec: CallSpec | (() => CallSpec);
  children: ReactNode;
  disabled?: boolean;
  title?: string;
  className?: string;
  onDone?(ok: boolean, value: unknown): void;
}

/** Button that signs one call and shows its result inline. */
export function TxButton({ spec, children, disabled, title, className, onDone }: Props) {
  const wallet = useWallet();
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<{ ok: boolean; text: string; hash?: string | null } | null>(null);

  async function run() {
    setBusy(true);
    setMsg(null);
    const res = await wallet.call(typeof spec === "function" ? spec() : spec);
    setBusy(false);
    setMsg(
      res.ok
        ? { ok: true, text: "Transaction sent.", hash: res.hash }
        : { ok: false, text: res.error ?? "Transaction failed." },
    );
    let value: unknown;
    if (res.ok) {
      value = decodeSuccessValue(res.outcome);
    }
    onDone?.(res.ok, value);
  }

  return (
    <div className="tx">
      <button className={className ?? "btn"} disabled={disabled || busy || !wallet.accountId} title={title} onClick={run}>
        {busy ? "Waiting for wallet…" : children}
      </button>
      {msg && (
        <p className={msg.ok ? "ok" : "bad"} role={msg.ok ? "status" : "alert"}>
          {msg.text}{" "}
          {msg.hash && (
            <a href={explorerTx(msg.hash)} target="_blank" rel="noreferrer">
              view
            </a>
          )}
        </p>
      )}
    </div>
  );
}
