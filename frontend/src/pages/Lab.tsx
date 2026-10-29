import { useRef, useState } from "react";
import { api } from "../api";
import { useLoad } from "../hooks";
import { useWallet } from "../wallet";
import { Card, ErrorBox } from "../components/ui";
import { StatusPanel } from "../components/StatusPanel";
import { TxButton } from "../components/TxButton";
import { advanceClock, bellCall, decodeSuccessValue } from "../lib/calls";
import { runPipeline } from "../lib/bell";
import { config } from "../lib/config";
import { formatNsDate } from "../lib/format";

export function Lab() {
  const wallet = useWallet();
  const status = useLoad(() => api.status(), []);
  const settings = useLoad(() => api.settings(), []);
  const [log, setLog] = useState<string[]>([]);
  const [running, setRunning] = useState(false);
  const stop = useRef(false);

  const s = settings.data;
  const day = s?.dayNs ?? null;
  const epoch = s && day != null ? day * BigInt(s.daysPerEpoch) : null;
  const notOwner = !!(s && wallet.accountId && s.owner !== wallet.accountId);

  async function run() {
    setRunning(true);
    stop.current = false;
    setLog([]);
    const push = (l: string) => setLog((x) => [...x, l]);
    const res = await runPipeline({
      getStatus: () => api.status().catch((e) => { push(`status failed: ${e}`); return null; }),
      call: async (m) => {
        const r = await wallet.call(bellCall(config.core, m));
        return { ok: r.ok, error: r.error, value: r.ok ? decodeSuccessValue(r.outcome) : undefined };
      },
      log: push,
      shouldStop: () => stop.current,
    });
    push(`done after ${res.calls} call(s): ${res.reason}`);
    setRunning(false);
    status.reload();
  }

  return (
    <>
      <Card title="Lab (staging only)">
        <p>
          The staging build of core has <code>advance_clock</code>, which moves core's clock forward so days and epochs end in
          minutes. It is <b>owner-only on chain</b>: {s ? <>owner is <code>{s.owner}</code></> : "loading owner…"}.
          {notOwner && <span className="alert info"> You are signed in as {wallet.accountId}, so these calls will be rejected.</span>}
        </p>
        <ErrorBox error={settings.error} />
        <p className="muted small">
          Day length {day != null ? `${Number(day / 1_000_000_000n)} s` : "?"}, {s?.daysPerEpoch ?? "?"} days per epoch.
          Core clock now: {formatNsDate(status.data?.now)}.
        </p>
        <div className="row">
          <TxButton spec={() => advanceClock(config.core, day ?? 0n)} disabled={day == null} onDone={() => status.reload()}>
            Advance 1 day
          </TxButton>
          <TxButton spec={() => advanceClock(config.core, epoch ?? 0n)} disabled={epoch == null} onDone={() => status.reload()}>
            Advance 1 epoch
          </TxButton>
        </div>
      </Card>

      <Card title="Run pipeline to now">
        <p>
          Calls <code>close_epoch</code>, <code>settle_day</code> and <code>scan_tick</code> (300 TGas each) while status says
          they are due. Each call is signed in your wallet; sign in with this core's access key so they do not prompt.
          Stops when nothing is due, a tick does nothing, or status stops changing (the X scan finishes asynchronously, so a
          second run may be needed).
        </p>
        <div className="row">
          <button className="btn primary" onClick={run} disabled={running || !wallet.accountId}>
            {running ? "Running…" : "Run pipeline"}
          </button>
          {running && <button className="btn" onClick={() => (stop.current = true)}>Stop</button>}
        </div>
        {log.length > 0 && <pre className="log" data-testid="pipeline-log">{log.join("\n")}</pre>}
      </Card>
      <StatusPanel status={status.data} error={status.error} loading={status.loading} reload={status.reload} />
    </>
  );
}
