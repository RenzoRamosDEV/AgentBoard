import type { BreakdownRow } from "../lib/api";
import { fmt } from "../lib/format";
import { Empty } from "./Panel";

export function ModelsTable({ rows }: { rows: BreakdownRow[] }) {
  if (!rows.length) return <Empty />;
  const total = rows.reduce((a, r) => a + r.costUsd, 0) || 1;
  return (
    <table className="table">
      <thead>
        <tr>
          <th>Modelo</th>
          <th className="num">Coste</th>
          <th className="num">% del total</th>
          <th className="num">Llamadas</th>
          <th className="num">Cache hit</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((r) => (
          <tr key={r.key}>
            <td>
              <code>{r.label}</code>
              {!r.hasPrice && <span className="badge" title="Sin precio conocido: cuenta con coste 0">sin precio</span>}
            </td>
            <td className="num">{fmt.usd(r.costUsd)}</td>
            <td className="num">{fmt.pct(r.costUsd / total)}</td>
            <td className="num">{fmt.int(r.calls)}</td>
            <td className="num">{fmt.pct(r.cacheHit)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
