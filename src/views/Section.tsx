import { activityLabel, fmt, modelName } from "../lib/format";
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
        { label: "Coste del periodo", value: fmt.usd(total), hint: `${days} días con actividad`, tone: "accent" },
        { label: "Media por día activo", value: fmt.usd(days ? total / days : 0), hint: `${fmt.int(calls(data.daily))} llamadas` },
        { label: "Día más caro", value: best ? fmt.usd(best.costUsd) : "–", hint: best ? fmt.date(best.ts) : "" },
      ];
    }
    case "agent": {
      const rows = data.agents;
      const t = top(rows);
      return [
        { label: "Coste total", value: fmt.usd(sum(rows)), hint: `${rows.length} agentes`, tone: "accent" },
        { label: "Llamadas", value: fmt.int(calls(rows)), hint: `${fmt.int(rows.reduce((a, r) => a + r.sessions, 0))} sesiones` },
        { label: "Agente principal", value: t?.label ?? "–", hint: t ? `${fmt.pct(sum(rows) ? t.costUsd / sum(rows) : 0)} del coste · ${fmt.int(t.calls)} llamadas` : "" },
      ];
    }
    case "project": {
      const rows = data.branches ?? data.projects;
      const t = top(rows);
      return [
        { label: "Coste total", value: fmt.usd(sum(rows)), hint: `${rows.length} ${data.branches ? "ramas" : "proyectos"}`, tone: "accent" },
        { label: "Sesiones", value: fmt.int(rows.reduce((a, r) => a + r.sessions, 0)), hint: "en el periodo" },
        { label: data.branches ? "Rama principal" : "Proyecto principal", value: t?.label ?? "–", hint: t ? `${fmt.usd(t.costUsd)} · ${fmt.pct(sum(rows) ? t.costUsd / sum(rows) : 0)}` : "" },
      ];
    }
    case "activity": {
      const rows = data.activity.activities;
      const turns = rows.reduce((a, r) => a + r.turns, 0);
      const edits = rows.reduce((a, r) => a + r.editTurns, 0);
      const ok = rows.reduce((a, r) => a + (r.oneShot ?? 0) * r.editTurns, 0);
      const t = top(rows);
      return [
        { label: "Coste total", value: fmt.usd(sum(rows)), hint: `${fmt.int(turns)} turnos · ${fmt.usd(turns ? sum(rows) / turns : 0)} por turno`, tone: "accent" },
        { label: "1-shot global", value: edits ? fmt.pct(ok / edits) : "–", hint: `de ${fmt.int(edits)} turnos con ediciones`, tone: edits && ok / edits >= 0.95 ? "good" : undefined },
        { label: "Actividad principal", value: t ? activityLabel(t.key) : "–", hint: t ? `${fmt.pct(sum(rows) ? t.costUsd / sum(rows) : 0)} del coste · ${fmt.int(t.turns)} turnos` : "" },
      ];
    }
    case "model": {
      const t = top(data.models);
      return [
        { label: "Coste total", value: fmt.usd(sum(data.models)), hint: `${data.models.length} modelos`, tone: "accent" },
        { label: "Llamadas", value: fmt.int(calls(data.models)), hint: "en el periodo" },
        { label: "Modelo principal", value: t ? modelName(t.key) : "–", hint: t ? `${fmt.usd(t.costUsd)} · cache hit ${fmt.pct(t.cacheHit)}` : "" },
      ];
    }
    case "tools":
    case "shell":
    case "mcp": {
      const rows = { tools: data.tools, shell: data.commands, mcp: data.mcp }[id];
      const errors = rows.reduce((a, r) => a + r.errors, 0);
      const t = [...rows].sort((a, b) => b.calls - a.calls)[0];
      return [
        { label: "Llamadas", value: fmt.int(calls(rows)), hint: `${rows.length} distintos`, tone: "accent" },
        { label: "Con error", value: calls(rows) ? fmt.pct(errors / calls(rows)) : "–", hint: `${fmt.int(errors)} llamadas`, tone: calls(rows) && errors / calls(rows) > 0.05 ? "warn" : undefined },
        { label: "Más usado", value: t?.label ?? "–", hint: t ? `${fmt.int(t.calls)} llamadas` : "" },
      ];
    }
    case "skills":
    case "agents": {
      const rows = id === "skills" ? data.skills : data.agentTypes;
      const t = top(rows);
      return [
        { label: "Coste", value: fmt.usd(sum(rows)), hint: `${fmt.int(calls(rows))} ${id === "skills" ? "usos" : "llamadas"}`, tone: "accent" },
        { label: id === "skills" ? "Skills y agentes" : "Tipos", value: fmt.int(rows.length), hint: "distintos en el periodo" },
        { label: "Más caro", value: t?.label ?? "–", hint: t ? fmt.usd(t.costUsd) : "" },
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
  const title = id === "project" && singleProject ? `By Branch · ${singleProject}` : s.title;
  return (
    <div className="main">
      <header className="page-head">
        <button className="link back" onClick={back}>
          ‹ Volver al resumen
        </button>
        <h1>
          {title} <span className="muted">· {s.question} · {periodLabel(period)}</span>
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
