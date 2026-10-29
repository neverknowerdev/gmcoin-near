import { api } from "../api";
import { useLoad } from "../hooks";
import { Card, ErrorBox, Loading } from "../components/ui";
import { StatusPanel } from "../components/StatusPanel";
import { chartGeometry } from "../lib/chart";
import { formatGm, gmToNumber, compact } from "../lib/format";
import type { Epoch } from "../lib/types";

const BOX = { width: 720, height: 300, left: 52, right: 16, top: 16, bottom: 32 };

function Chart({ epochs }: { epochs: Epoch[] }) {
  const g = chartGeometry(epochs, BOX);
  const base = BOX.height - BOX.bottom;
  return (
    <svg viewBox={`0 0 ${BOX.width} ${BOX.height}`} className="chart" role="img" aria-label="Weekly budget and minted GM per epoch" data-testid="budget-chart">
      {g.ticks.map((t, i) => (
        <g key={i}>
          <line x1={BOX.left} x2={BOX.width - BOX.right} y1={t.y} y2={t.y} className="grid" />
          <text x={BOX.left - 6} y={t.y + 4} textAnchor="end" className="axis">{t.label}</text>
        </g>
      ))}
      {g.bars.map((b) => (
        <rect key={b.epoch} x={b.x} y={b.y} width={b.w} height={Math.max(b.h, 0)} className="bar" data-testid="budget-bar">
          <title>{`Epoch ${b.epoch}: budget ${compact(b.value)} GM`}</title>
        </rect>
      ))}
      {g.line.length > 1 && <polyline className="line" fill="none" points={g.line.map((p) => `${p.x},${p.y}`).join(" ")} />}
      {g.line.map((p) => (
        <circle key={p.epoch} cx={p.x} cy={p.y} r={4} className="dot">
          <title>{`Epoch ${p.epoch}: minted ${compact(p.value)} GM`}</title>
        </circle>
      ))}
      {g.labels.map((l) => (
        <text key={l.label} x={l.x} y={base + 18} textAnchor="middle" className="axis">{l.label}</text>
      ))}
      <text x={BOX.left} y={BOX.height - 2} className="axis">epoch</text>
    </svg>
  );
}

export function Budget() {
  const status = useLoad(() => api.status(), []);
  const hist = useLoad(() => api.epochHistory(), []);
  return (
    <>
      <StatusPanel status={status.data} error={status.error} loading={status.loading} reload={status.reload} />
      <Card title="Weekly budget and minted" actions={<button className="btn link" onClick={hist.reload}>refresh</button>}>
        <ErrorBox error={hist.error} />
        {hist.loading && !hist.data && <Loading what="epoch history" />}
        {hist.data && hist.data.length === 0 && <p className="muted">No epochs yet.</p>}
        {hist.data && hist.data.length > 0 && (
          <>
            <div className="legend">
              <span><i className="swatch bar" /> Budget</span>
              <span><i className="swatch line" /> Minted</span>
            </div>
            <Chart epochs={hist.data} />
            <div className="table-wrap">
              <table className="table" data-testid="epoch-table">
                <thead>
                  <tr>
                    <th>Epoch</th>
                    <th>Start day</th>
                    <th className="num">Budget (GM)</th>
                    <th className="num">Minted (GM)</th>
                    <th className="num">Active users</th>
                    <th>State</th>
                  </tr>
                </thead>
                <tbody>
                  {hist.data.map((e) => (
                    <tr key={e.index}>
                      <td>{e.index}</td>
                      <td>{e.startDay}</td>
                      <td className="num">{formatGm(e.budget, 2)}</td>
                      <td className="num">{formatGm(e.minted, 2)}</td>
                      <td className="num">{e.closed ? e.activeUsers : "-"}</td>
                      <td>{e.closed ? "closed" : "open"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <p className="muted small">
              Latest budget {formatGm(hist.data[hist.data.length - 1].budget, 2)} GM (about {compact(gmToNumber(hist.data[hist.data.length - 1].dailyPool))} GM a day).
            </p>
          </>
        )}
      </Card>
    </>
  );
}
