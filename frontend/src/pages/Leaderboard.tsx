import { useState } from "react";
import { api } from "../api";
import { useLoad } from "../hooks";
import { Card, ErrorBox, Loading } from "../components/ui";
import { shortAccount } from "../lib/format";
import { profileUrl } from "../lib/x";

export function Leaderboard() {
  const status = useLoad(() => api.status(), []);
  const current = status.data?.currentEpoch ?? null;
  const [picked, setPicked] = useState<number | null>(null);
  const epoch = picked ?? current;
  const rows = useLoad(() => (epoch == null ? Promise.resolve(null) : api.leaderboard(epoch)), [epoch]);

  const epochs = current == null ? [] : Array.from({ length: current + 1 }, (_, i) => current - i);

  return (
    <Card
      title="Leaderboard"
      actions={
        epoch != null && (
          <label className="inline">
            Epoch{" "}
            <select value={epoch} onChange={(e) => setPicked(Number(e.target.value))} aria-label="Epoch">
              {epochs.map((e) => (
                <option key={e} value={e}>
                  {e}
                  {e === current ? " (current)" : ""}
                </option>
              ))}
            </select>
          </label>
        )
      }
    >
      <ErrorBox error={status.error ?? rows.error} />
      {(status.loading || rows.loading) && !rows.data && <Loading what="leaderboard" />}
      {rows.data && rows.data.length === 0 && <p className="muted" data-testid="lb-empty">No scored users in epoch {epoch} yet.</p>}
      {rows.data && rows.data.length > 0 && (
        <table className="table" data-testid="lb-table">
          <thead>
            <tr>
              <th>#</th>
              <th>Account</th>
              <th>X</th>
              <th className="num">Points</th>
            </tr>
          </thead>
          <tbody>
            {rows.data.map((r, i) => (
              <tr key={r.account}>
                <td>{i + 1}</td>
                <td title={r.account}>{shortAccount(r.account, 32)}</td>
                <td>
                  {r.handle ? (
                    <a href={profileUrl(r.handle)} target="_blank" rel="noreferrer">
                      @{r.handle}
                    </a>
                  ) : (
                    <span className="muted">-</span>
                  )}
                </td>
                <td className="num">{r.points.toLocaleString("en-US")}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Card>
  );
}
