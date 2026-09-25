import { activityLabel, fmt, modelName } from "../lib/format";
import { t } from "../lib/i18n";
import type { Period } from "../lib/period";
import { sectionOf, type SectionId } from "../lib/sections";
import type { DashboardData } from "../lib/useData";
import { Kpis, type Kpi } from "../components/Kpis";
import { periodLabel } from "./Overview";
import { DailyFull } from "./panels";
import { ActivityFull, AgentFull, AgentTypesFull, McpFull, ModelFull, ProjectFull, ShellFull, SkillsFull, ToolsFull } from "./sections";

/** Tres cifras clave de cada apartado. */
function sectionKpis(id: Exclude<SectionId, "overview">, data: DashboardData): Kpi[] {
  const top = <T,>(rows: T[]): T | undefined => rows[0];
  const sum = (rows: { costUsd: number }[]) => rows.reduce((a, r) => a + r.costUsd, 0);
  const calls = (rows: { calls: number }[]) => rows.reduce((a, r) => a + r.calls, 0);
  switch (id) {
    case "daily": {
      const days = data.daily.length;
      const total = sum(data.daily);
      const best = [...data.daily].sort((a, b) => b.costUsd - a.costUsd)[0];
      return [
        { label: t("Coste del periodo"), value: fmt.usd(total), hint: t("{n} días con actividad", { n: days }), tone: "accent" },
        { label: t("Media por día activo"), value: fmt.usd(days ? total / days : 0), hint: t("{n} llamadas", { n: fmt.int(calls(data.daily)) }) },
        { label: t("Día más caro"), value: best ? fmt.usd(best.costUsd) : "–", hint: best ? fmt.date(best.ts) : "" },
      ];
    }
    case "agent": {
      const rows = data.agents;
      const top_ = top(rows);
      return [
        { label: t("Coste total"), value: fmt.usd(sum(rows)), hint: t("{n} agentes", { n: rows.length }), tone: "accent" },
        { label: t("Llamadas"), value: fmt.int(calls(rows)), hint: t("{n} sesiones", { n: fmt.int(rows.reduce((a, r) => a + r.sessions, 0)) }) },
        { label: t("Agente principal"), value: top_?.label ?? "–", hint: top_ ? t("{p} del coste · {n} llamadas", { p: fmt.pct(sum(rows) ? top_.costUsd / sum(rows) : 0), n: fmt.int(top_.calls) }) : "" },
      ];
    }
    case "project": {
      const rows = data.branches ?? data.projects;
      const top_ = top(rows);
      return [
        { label: t("Coste total"), value: fmt.usd(sum(rows)), hint: t(data.branches ? "{n} ramas" : "{n} proyectos", { n: rows.length }), tone: "accent" },
        { label: t("Sesiones"), value: fmt.int(rows.reduce((a, r) => a + r.sessions, 0)), hint: t("en el periodo") },
        { label: t(data.branches ? "Rama principal" : "Proyecto principal"), value: top_?.label ?? "–", hint: top_ ? `${fmt.usd(top_.costUsd)} · ${fmt.pct(sum(rows) ? top_.costUsd / sum(rows) : 0)}` : "" },
      ];
    }
    case "activity": {
      const rows = data.activity.activities;
      const turns = rows.reduce((a, r) => a + r.turns, 0);
      const edits = rows.reduce((a, r) => a + r.editTurns, 0);
      const ok = rows.reduce((a, r) => a + (r.oneShot ?? 0) * r.editTurns, 0);
      const top_ = top(rows);
      return [
        { label: t("Coste total"), value: fmt.usd(sum(rows)), hint: t("{n} turnos · {v} por turno", { n: fmt.int(turns), v: fmt.usd(turns ? sum(rows) / turns : 0) }), tone: "accent" },
        { label: t("1-shot global"), value: edits ? fmt.pct(ok / edits) : "–", hint: t("de {n} turnos con ediciones", { n: fmt.int(edits) }), tone: edits && ok / edits >= 0.95 ? "good" : undefined },
        { label: t("Actividad principal"), value: top_ ? activityLabel(top_.key) : "–", hint: top_ ? t("{p} del coste · {n} turnos", { p: fmt.pct(sum(rows) ? top_.costUsd / sum(rows) : 0), n: fmt.int(top_.turns) }) : "" },
      ];
    }
    case "model": {
      const top_ = top(data.models);
      return [
        { label: t("Coste total"), value: fmt.usd(sum(data.models)), hint: t("{n} modelos", { n: data.models.length }), tone: "accent" },
        { label: t("Llamadas"), value: fmt.int(calls(data.models)), hint: t("en el periodo") },
        { label: t("Modelo principal"), value: top_ ? modelName(top_.key) : "–", hint: top_ ? t("{v} · cache hit {p}", { v: fmt.usd(top_.costUsd), p: fmt.pct(top_.cacheHit) }) : "" },
      ];
    }
    case "tools":
    case "shell":
    case "mcp": {
      const rows = { tools: data.tools, shell: data.commands, mcp: data.mcp }[id];
      const errors = rows.reduce((a, r) => a + r.errors, 0);
      const top_ = [...rows].sort((a, b) => b.calls - a.calls)[0];
      return [
        { label: t("Llamadas"), value: fmt.int(calls(rows)), hint: t("{n} distintos", { n: rows.length }), tone: "accent" },
        { label: t("Con error"), value: calls(rows) ? fmt.pct(errors / calls(rows)) : "–", hint: t("{n} llamadas", { n: fmt.int(errors) }), tone: calls(rows) && errors / calls(rows) > 0.05 ? "warn" : undefined },
        { label: t("Más usado"), value: top_?.label ?? "–", hint: top_ ? t("{n} llamadas", { n: fmt.int(top_.calls) }) : "" },
      ];
    }
    case "skills":
    case "agents": {
      const rows = id === "skills" ? data.skills : data.agentTypes;
      const top_ = top(rows);
      return [
        { label: t("Coste"), value: fmt.usd(sum(rows)), hint: t(id === "skills" ? "{n} usos" : "{n} llamadas", { n: fmt.int(calls(rows)) }), tone: "accent" },
        { label: t(id === "skills" ? "Skills y agentes" : "Tipos"), value: fmt.int(rows.length), hint: t("distintos en el periodo") },
        { label: t("Más caro"), value: top_?.label ?? "–", hint: top_ ? fmt.usd(top_.costUsd) : "" },
      ];
    }
  }
}

export function Section({
  id,
  data,
  period,
  singleProject,
  back,
}: {
  id: Exclude<SectionId, "overview">;
  data: DashboardData;
  period: Period;
  singleProject: string | null;
  back: () => void;
}) {
  const s = sectionOf(id);
  const title = id === "project" && singleProject ? t("By Branch · {name}", { name: singleProject }) : t(s.title);
  return (
    <div className="main">
      <header className="page-head">
        <button className="link back" onClick={back}>
          {t("‹ Volver al resumen")}
        </button>
        <h1>
          {title} <span className="muted">· {t(s.question)} · {periodLabel(period)}</span>
        </h1>
      </header>
      <Kpis items={sectionKpis(id, data)} columns={3} />
      {id === "daily" && <DailyFull data={data} />}
      {id === "agent" && <AgentFull data={data} />}
      {id === "project" && <ProjectFull data={data} singleProject={singleProject} />}
      {id === "activity" && <ActivityFull data={data} />}
      {id === "model" && <ModelFull data={data} />}
      {id === "tools" && <ToolsFull data={data} />}
      {id === "shell" && <ShellFull data={data} />}
      {id === "skills" && <SkillsFull data={data} />}
      {id === "mcp" && <McpFull data={data} />}
      {id === "agents" && <AgentTypesFull data={data} />}
    </div>
  );
}
