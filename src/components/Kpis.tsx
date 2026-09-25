export interface Kpi {
  label: string;
  value: string;
  hint?: string;
  tone?: "accent" | "good" | "warn";
}

export const Kpis = ({ items, columns }: { items: Kpi[]; columns?: number }) => (
  <div className="kpis" style={columns ? { gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))` } : undefined}>
    {items.map((k) => (
      <div className="kpi" key={k.label}>
        <span className="kpi-label">{k.label}</span>
        <strong className={`kpi-value ${k.tone ?? ""}`}>{k.value}</strong>
        {k.hint && <span className="kpi-hint">{k.hint}</span>}
      </div>
    ))}
  </div>
);
