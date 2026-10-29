import { api } from "../api";
import { useLoad } from "../hooks";
import { useWallet } from "../wallet";
import { Card } from "../components/ui";
import { StatusPanel } from "../components/StatusPanel";
import { TxButton } from "../components/TxButton";
import { bellCall, type BellMethod } from "../lib/calls";
import { bellState } from "../lib/bell";
import { config } from "../lib/config";
import { formatGm } from "../lib/format";

const BELLS: { method: BellMethod; label: string; text: string }[] = [
  { method: "scan_tick", label: "Scan", text: "Starts the next X scan batch for the oldest unscanned day." },
  { method: "settle_day", label: "Settle", text: "Settles the oldest scanned day and mints its rewards (or retries a failed mint)." },
  { method: "close_epoch", label: "Close epoch", text: "Closes the finished week and sets next week's budget." },
];

export function Bell() {
  const wallet = useWallet();
  const status = useLoad(() => api.status(), []);
  const settings = useLoad(() => api.settings(), []);
  const state = bellState(status.data);
  return (
    <>
      <Card title="Ring the GM bell">
        <p>
          NEAR has no cron, so anyone can push the pipeline along. Each button below does one step only when it is due, and the
          caller earns a GM tip
          {settings.data ? ` (${formatGm(settings.data.tipAmount)} GM per step that did work, up to ${formatGm(settings.data.tipDailyLimit)} GM a day in total)` : ""}
          . An early or repeated call does nothing but cost a little gas. Each call attaches 300 TGas; unused gas is refunded.
        </p>
        {!wallet.accountId && (
          <p>
            <button className="btn primary" onClick={wallet.signIn} disabled={config.errors.length > 0}>
              Connect wallet
            </button>
          </p>
        )}
        <div className="bells">
          {BELLS.map((b) => (
            <div key={b.method} className="bell">
              <h3>{b.label}</h3>
              <p className="small">{b.text}</p>
              <TxButton
                className="btn primary"
                spec={bellCall(config.core, b.method)}
                disabled={!state[b.method]}
                title={state[b.method] ? "Due now" : "Not due"}
                onDone={() => status.reload()}
              >
                {b.method}
              </TxButton>
              <p className="muted small">{state[b.method] ? "Due now" : "Not due"}</p>
            </div>
          ))}
        </div>
      </Card>
      <StatusPanel status={status.data} error={status.error} loading={status.loading} reload={status.reload} />
    </>
  );
}
