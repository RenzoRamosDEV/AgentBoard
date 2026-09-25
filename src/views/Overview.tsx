import type { ReactElement } from "react";
import { fmt } from "../lib/format";
import { PERIODS, projectMonth, type Period } from "../lib/period";
import { SECTIONS, type SectionId } from "../lib/sections";
import type { DashboardData } from "../lib/useData";
import { Kpis, type Kpi } from "../components/Kpis";
import { Panel } from "../components/Panel";
import { ActivityPanel, AgentTypesPanel, DailyPanel, McpPanel, ModelPanel, ProjectPanel, ShellPanel, SkillsPanel, ToolsPanel, type PanelProps } from "./panels";

export const PANELS: Record<Exclude<SectionId, "overview">, (p: PanelProps) => ReactElement> = {
  daily: DailyPanel,
  project: ProjectPanel,
  activity: ActivityPanel,
  model: ModelPanel,
  tools: ToolsPanel,
  shell: ShellPanel,
  skills: SkillsPanel,
  mcp: McpPanel,
  agents: AgentTypesPanel,
};

export function periodLabel(period: Period) {
  if (period.kind === "custom" && period.start && period.end) return `${period.start} → ${period.end}`;
  return PERIODS.find((p) => p.kind === period.kind)?.label.toLowerCase() ?? "";
}

export function summaryKpis(data: DashboardData, budget: number | null): Kpi[] {
  const { summary: s } = data;
  const monthSpent = data.month.reduce((a, p) => a + p.costUsd, 0);
  const projection = projectMonth(monthSpent);
  const budgetHint = budget != null ? ` · presupuesto ${fmt.usd(budget)} (${fmt.pct(budget ? monthSpent / budget : 0)})` : "";
  return [
    { label: "Coste", value: fmt.usd(s.costUsd), hint: `${fmt.int(s.calls)} llamadas`, tone: "accent" },
    { label: "Sesiones", value: fmt.int(s.sessions), hint: s.sessions ? `${fmt.usd(s.costUsd / s.sessions)} por sesión` : "" },
    { label: "Cache hit", value: fmt.pct(s.cacheHit), hint: `${fmt.compact(s.cacheRead)} leídos · ${fmt.compact(s.cacheWrite)} escritos` },
    { label: "Ahorro por caché", value: fmt.usd(s.cacheSavingsUsd), hint: "frente a pagar esa entrada sin caché", tone: "good" },
    { label: "Burn rate", value: `${fmt.usd(s.burnRateUsdH)}/h`, hint: "últimos 60 minutos" },
    {
      label: "Gasto del mes",
      value: fmt.usd(monthSpent),
      hint: `proyección ${fmt.usd(projection)}${budgetHint}`,
      tone: budget != null && projection > budget ? "warn" : undefined,
    },
  ];
}

export function Overview({
  data,
  period,
  budget,
  singleProject,
  open,
}: {
  data: DashboardData;
  period: Period;
  budget: number | null;
  singleProject: string | null;
  open: (s: SectionId) => void;
}) {
  const scope = singleProject ? `proyecto ${singleProject}` : "todos los agentes y proyectos";
  const monthSpent = data.month.reduce((a, p) => a + p.costUsd, 0);
  const over = budget != null && projectMonth(monthSpent) > budget;
  return (
    <div className="main">
      <header className="page-head">
        <h1>
          Resumen <span className="muted">· {periodLabel(period)} · {scope}</span>
        </h1>
      </header>
      {data.summary.calls === 0 && (
        <div className="notice">No hay llamadas en este periodo. Si acabas de instalar la app, espera a que termine el escaneo inicial o elige "Todo".</div>
      )}
      <Kpis items={summaryKpis(data, budget)} />
      {over && <div className="notice warn">⚠ La proyección del mes supera el presupuesto de {fmt.usd(budget!)}.</div>}
      {data.summary.unpricedModels.length > 0 && (
        <p className="muted small">Modelos sin precio (cuentan como $0): {data.summary.unpricedModels.join(", ")}</p>
      )}
      <Panel id="daily" title="Daily Activity" question="¿Cuánto gasto cada día?" onOpen={() => open("daily")} wide>
        <DailyPanel data={data} full={false} singleProject={singleProject} open={() => open("daily")} />
      </Panel>
      <div className="grid-2">
        {SECTIONS.filter((s) => s.id !== "overview" && s.id !== "daily").map((s) => {
          const Body = PANELS[s.id as Exclude<SectionId, "overview">];
          const title = s.id === "project" && singleProject ? `By Branch · ${singleProject}` : s.title;
          return (
            <Panel key={s.id} id={s.id} title={title} question={s.question} onOpen={() => open(s.id)}>
              <Body data={data} full={false} singleProject={singleProject} open={() => open(s.id)} />
            </Panel>
          );
        })}
      </div>
    </div>
  );
}
