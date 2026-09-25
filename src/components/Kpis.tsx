import type { Summary } from "../lib/api";
import { fmt } from "../lib/format";

export function Kpis({ s }: { s: Summary }) {
  const tiles = [
    { label: "Coste", value: fmt.usd(s.costUsd), hint: `${fmt.compact(s.inputTokens + s.cacheRead + s.cacheWrite)} tokens de entrada · ${fmt.compact(s.outputTokens)} de salida` },
    { label: "Llamadas", value: fmt.int(s.calls), hint: "respuestas del modelo" },
    { label: "Sesiones", value: fmt.int(s.sessions), hint: s.calls ? `${fmt.usd(s.costUsd / Math.max(s.sessions, 1))} por sesión` : "" },
    { label: "Cache hit", value: fmt.pct(s.cacheHit), hint: "entrada servida desde caché" },
    { label: "Ahorro por caché", value: fmt.usd(s.cacheSavingsUsd), hint: "frente a pagar esa entrada sin caché" },
    { label: "Burn rate", value: `${fmt.usd(s.burnRateUsdH)}/h`, hint: "últimos 60 minutos" },
  ];
  return (
    <div className="kpis">
      {tiles.map((t) => (
        <div className="kpi" key={t.label}>
          <span className="kpi-label">{t.label}</span>
          <strong>{t.value}</strong>
          <span className="kpi-hint">{t.hint}</span>
        </div>
      ))}
    </div>
  );
}
