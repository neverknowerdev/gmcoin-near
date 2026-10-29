import { Card, ErrorBox, Loading, Stat } from "./ui";
import { formatGm, formatNsDate } from "../lib/format";
import type { Status } from "../lib/types";
import { dueSummary } from "../lib/bell";

export function StatusPanel({ status, error, loading, reload }: { status: Status | null; error: string | null; loading: boolean; reload(): void }) {
  return (
    <Card title="Status" actions={<button className="btn link" onClick={reload}>refresh</button>}>
      <ErrorBox error={error} />
      {loading && !status && <Loading what="status" />}
      {status && (
        <>
          <div className="stats" data-testid="status">
            <Stat label="Today (day)" value={status.today} hint={formatNsDate(status.now)} />
            <Stat label="Epoch" value={status.currentEpoch} testId="status-epoch" />
            <Stat label="Weekly budget" value={`${formatGm(status.budget, 2)} GM`} />
            <Stat label="Daily pool" value={`${formatGm(status.dailyPool, 2)} GM`} />
            <Stat label="Total minted" value={`${formatGm(status.totalMinted, 2)} GM`} />
            <Stat label="Users / linked" value={`${status.users} / ${status.linked}`} />
          </div>
          <ul className="due" aria-label="Due work">
            <li className={status.scanDue ? "on" : ""}>Scan day {status.scanDay}: {status.scanDue ? "due" : "not due"}</li>
            <li className={status.settleDue ? "on" : ""}>Settle day {status.settleDay}: {status.settleDue ? "due" : "not due"}</li>
            <li className={status.closeDue ? "on" : ""}>Close epoch {status.currentEpoch}: {status.closeDue ? "due" : "not due"}</li>
          </ul>
          <p className="muted small">{dueSummary(status)}</p>
          {status.paused && <p className="alert error">Core is paused. Ticks and registrations are rejected.</p>}
          {status.failedMints > 0 && <p className="alert info">{status.failedMints} failed mint batch(es) waiting for a settle retry.</p>}
        </>
      )}
    </Card>
  );
}
