import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getSummary, type Summary } from "./lib/api";

const usd = new Intl.NumberFormat("es-ES", { style: "currency", currency: "USD" });
const num = new Intl.NumberFormat("es-ES");

export default function App() {
  const [summary, setSummary] = useState<Summary | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = () => getSummary().then(setSummary).catch((e) => setError(String(e)));

  useEffect(() => {
    load();
    const off = listen("ingest://done", load);
    return () => {
      off.then((f) => f());
    };
  }, []);

  if (error) return <main className="empty">Error: {error}</main>;
  if (!summary) return <main className="empty">Cargando…</main>;

  return (
    <main>
      <h1>AgentBurn</h1>
      <section className="kpis">
        <Kpi label="Coste" value={usd.format(summary.costUsd)} />
        <Kpi label="Llamadas" value={num.format(summary.calls)} />
        <Kpi label="Sesiones" value={num.format(summary.sessions)} />
        <Kpi label="Tokens de salida" value={num.format(summary.outputTokens)} />
      </section>
      {summary.firstTs && (
        <p className="muted">Primer registro: {new Date(summary.firstTs).toLocaleString()}</p>
      )}
    </main>
  );
}

function Kpi({ label, value }: { label: string; value: string }) {
  return (
    <div className="kpi">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}
