import type { ReactElement } from "react";
import { t } from "../lib/i18n";
import { fmt } from "../lib/format";
import { PERIODS, projectMonth, type Period } from "../lib/period";
import { SECTIONS, type SectionId } from "../lib/sections";
import type { DashboardData } from "../lib/useData";
import { Kpis, type Kpi } from "../components/Kpis";
import { Panel } from "../components/Panel";
import { ActivityPanel, AgentPanel, AgentTypesPanel, DailyPanel, McpPanel, ModelPanel, ProjectPanel, ShellPanel, SkillsPanel, ToolsPanel, type PanelProps } from "./panels";

export const PANELS: Record<Exclude<SectionId, "overview">, (p: PanelProps) => ReactElement> = {
  daily: DailyPanel,
  agent: AgentPanel,
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
  return t(PERIODS.find((p) => p.kind === period.kind)?.label ?? "").toLowerCase();
}

export function summaryKpis(data: DashboardData, budget: number | null): Kpi[] {
  const { summary: s } = data;
  const monthSpent = data.month.reduce((a, p) => a + p.costUsd, 0);
  const projection = projectMonth(monthSpent);
  const budgetHint = budget != null ? t(" · presupuesto {b} ({p})", { b: fmt.usd(budget), p: fmt.pct(budget ? monthSpent / budget : 0) }) : "";
  return [
    { label: t("Coste"), value: fmt.usd(s.costUsd), hint: t("{n} llamadas", { n: fmt.int(s.calls) }), tone: "accent" },
    { label: t("Sesiones"), value: fmt.int(s.sessions), hint: s.sessions ? t("{v} por sesión", { v: fmt.usd(s.costUsd / s.sessions) }) : "" },
    { label: t("Cache hit"), value: fmt.pct(s.cacheHit), hint: t("{r} leídos · {w} escritos", { r: fmt.compact(s.cacheRead), w: fmt.compact(s.cacheWrite) }) },
    { label: t("Ahorro por caché"), value: fmt.usd(s.cacheSavingsUsd), hint: t("estimado: esa entrada a precio normal"), tone: "good" },
    { label: t("Burn rate"), value: `${fmt.usd(s.burnRateUsdH)}/h`, hint: t("últimos 60 minutos") },
    {
      label: t("Gasto del mes"),
      value: fmt.usd(monthSpent),
      hint: t("proyección {v}", { v: fmt.usd(projection) }) + budgetHint,
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
  const scope = singleProject ? t("proyecto {name}", { name: singleProject }) : t("todos los agentes y proyectos");
  const monthSpent = data.month.reduce((a, p) => a + p.costUsd, 0);
  const over = budget != null && projectMonth(monthSpent) > budget;
  return (
    <div className="main">
      <header className="page-head">
        <h1>
          {t("Resumen")} <span className="muted">· {periodLabel(period)} · {scope}</span>
        </h1>
      </header>
      {data.summary.calls === 0 && (
        <div className="notice">{t('No hay llamadas en este periodo. Si acabas de instalar la app, espera a que termine el escaneo inicial o elige "Todo".')}</div>
      )}
      <Kpis items={summaryKpis(data, budget)} />
      {over && <div className="notice warn">{t("⚠ La proyección del mes supera el presupuesto de {b}.", { b: fmt.usd(budget!) })}</div>}
      <div className="grid-top">
        <Panel id="daily" title="Daily Activity" question={t("¿Cuánto gasto cada día?")} onOpen={() => open("daily")}>
          <DailyPanel data={data} full={false} singleProject={singleProject} />
        </Panel>
        <Panel id="agent" title="By Agent" question={t("¿Qué agente uso más?")} onOpen={() => open("agent")}>
          <AgentPanel data={data} full={false} singleProject={singleProject} />
        </Panel>
      </div>
      <div className="grid-2">
        {SECTIONS.filter((s) => !["overview", "daily", "agent"].includes(s.id)).map((s) => {
          const Body = PANELS[s.id as Exclude<SectionId, "overview">];
          const title = s.id === "project" && singleProject ? t("By Branch · {name}", { name: singleProject }) : t(s.title);
          return (
            <Panel key={s.id} id={s.id} title={title} question={t(s.question)} onOpen={() => open(s.id)}>
              <Body data={data} full={false} singleProject={singleProject} />
            </Panel>
          );
        })}
      </div>
    </div>
  );
}
