import { useEffect, useState } from "react";
import { api, type ActivityReport, type BreakdownRow, type Filter, type Point, type Summary } from "../lib/api";
import { ACTIVITIES, fmt, modelName } from "../lib/format";
import { monthStart, projectMonth, type Period, PERIODS } from "../lib/period";
import { Cost, OneShot, TermPanel, TermTable } from "../components/Term";

interface Data {
  summary: Summary;
  daily: Point[];
  month: Point[];
  projects: BreakdownRow[];
  branches: BreakdownRow[] | null;
  models: BreakdownRow[];
  activity: ActivityReport;
  tools: BreakdownRow[];
  commands: BreakdownRow[];
  skills: BreakdownRow[];
  mcp: BreakdownRow[];
  agentTypes: BreakdownRow[];
}

const C = {
  header: "var(--c-orange)",
  daily: "var(--c-blue)",
  project: "var(--c-green)",
  activity: "var(--c-yellow)",
  model: "var(--c-magenta)",
  tools: "var(--c-cyan)",
  shell: "var(--c-orange)",
  skills: "var(--c-violet)",
  mcp: "var(--c-pink)",
  agents: "var(--c-violet)",
};

const day = (ts: number) => {
  const d = new Date(ts);
  return `${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
};

const calls = (r: BreakdownRow) => fmt.int(r.calls);
const usesTooltip = (r: BreakdownRow) => (
  <>
    <b>{r.label}</b>
    <div>{fmt.int(r.calls)} usos</div>
    <div className="muted">
      {fmt.int(r.errors)} con error ({fmt.pct(r.calls ? r.errors / r.calls : 0)})
    </div>
  </>
);

export function Dashboard({
  filter,
  period,
  singleProject,
  budget,
  refresh,
}: {
  filter: Filter;
  period: Period;
  /** Nombre del proyecto si hay exactamente uno seleccionado. */
  singleProject: string | null;
  budget: number | null;
  refresh: number;
}) {
  const [data, setData] = useState<Data | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    // Gasto del mes: respeta agentes y proyectos, ignora el periodo.
    const monthFilter: Filter = { ...filter, from: monthStart().getTime(), to: undefined };
    const by = (k: Parameters<typeof api.breakdown>[1]) => api.breakdown(filter, k);
    Promise.all([
      api.summary(filter),
      api.timeseries(filter, "day"),
      api.timeseries(monthFilter, "day"),
      by("project"),
      singleProject ? by("branch") : Promise.resolve(null),
      by("model"),
      api.activity(filter),
      by("tool"),
      by("command"),
      by("skill"),
      by("mcp"),
      by("agent_type"),
    ])
      .then(([summary, daily, month, projects, branches, models, activity, tools, commands, skills, mcp, agentTypes]) => {
        if (!alive) return;
        setData({ summary, daily, month, projects, branches, models, activity, tools, commands, skills, mcp, agentTypes });
        setError(null);
      })
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
  }, [filter, singleProject, refresh]);

  if (error) return <div className="main term-error">No se pudieron cargar los datos: {error}</div>;
  if (!data) return <div className="main muted">Cargando…</div>;
  const { summary: s } = data;
  const monthSpent = data.month.reduce((a, p) => a + p.costUsd, 0);
  const projection = projectMonth(monthSpent);
  const periodLabel = PERIODS.find((p) => p.kind === period.kind)?.label ?? "";
  const oneShotByModel = new Map(data.activity.models.map((m) => [m.model, m.oneShot]));
  const byProject = data.branches ?? data.projects;

  return (
    <div className="main">
      <TermPanel title={<>AgentBurn <span className="muted">{periodLabel}</span></>} color={C.header} className="span-2 term-header">
        <p>
          <span className="cost">{fmt.usd(s.costUsd)}</span> cost <b>{fmt.int(s.calls)}</b> calls <b>{fmt.int(s.sessions)}</b> sessions{" "}
          <b>{fmt.pct(s.cacheHit)}</b> cache hit
        </p>
        <p className="muted">
          {fmt.compact(s.inputTokens)} in {fmt.compact(s.outputTokens)} out {fmt.compact(s.cacheRead)} cached {fmt.compact(s.cacheWrite)} written
        </p>
        <p className="muted">
          mes <span className="cost">{fmt.usd(monthSpent)}</span> · proyección <span className="cost">{fmt.usd(projection)}</span>
          {budget != null && (
            <>
              {" "}· presupuesto {fmt.usd(budget)} ({fmt.pct(budget ? monthSpent / budget : 0)})
            </>
          )}{" "}
          · burn <span className="cost">{fmt.usd(s.burnRateUsdH)}/h</span> · ahorro por caché <span className="good">{fmt.usd(s.cacheSavingsUsd)}</span>
        </p>
        {budget != null && projection > budget && <p className="warn">⚠ La proyección del mes supera el presupuesto de {fmt.usd(budget)}</p>}
        {s.unpricedModels.length > 0 && <p className="muted">sin precio (cuentan como $0): {s.unpricedModels.join(", ")}</p>}
      </TermPanel>

      <div className="term-grid">
        <TermPanel
          title={<>Daily Activity {data.daily.length > 14 && <span className="muted">· últimos 14 días con actividad</span>}</>}
          color={C.daily}
        >
          <TermTable
            rows={data.daily.slice(-14)}
            rowKey={(p) => String(p.ts)}
            value={(p) => p.costUsd}
            label={(p) => <span className="muted">{day(p.ts)}</span>}
            columns={[
              { header: "cost", cell: (p) => <Cost v={p.costUsd} /> },
              { header: "calls", cell: (p) => fmt.int(p.calls) },
            ]}
            limit={14}
            tooltip={(p) => (
              <>
                <b>{fmt.date(p.ts)}</b>
                <div>{fmt.usd(p.costUsd)}</div>
                <div className="muted">{fmt.int(p.calls)} llamadas</div>
              </>
            )}
          />
        </TermPanel>

        <TermPanel title={data.branches ? `By Branch · ${singleProject}` : "By Project"} color={C.project}>
          <TermTable
            rows={byProject}
            rowKey={(r) => r.key}
            value={(r) => r.costUsd}
            label={(r) => r.label}
            columns={[
              { header: "cost", cell: (r) => <Cost v={r.costUsd} /> },
              { header: "avg/s", cell: (r) => <Cost v={r.sessions ? r.costUsd / r.sessions : 0} /> },
              { header: "sess", cell: (r) => fmt.int(r.sessions) },
              { header: "overhead", cell: (r) => <span className="accent">{r.overheadTokens ? fmt.compact(r.overheadTokens) : "-"}</span> },
            ]}
            tooltip={(r) => (
              <>
                <b>{r.label}</b>
                <div>{fmt.usd(r.costUsd)} en {fmt.int(r.sessions)} sesiones</div>
                <div className="muted">overhead: tokens de contexto de la primera llamada de cada sesión</div>
              </>
            )}
          />
        </TermPanel>

        <TermPanel title="By Activity" color={C.activity}>
          <TermTable
            rows={data.activity.activities}
            rowKey={(r) => r.key}
            value={(r) => r.costUsd}
            label={(r) => {
              const a = ACTIVITIES[r.key] ?? { label: r.key, color: "inherit" };
              return <span style={{ color: a.color }}>{a.label}</span>;
            }}
            columns={[
              { header: "cost", cell: (r) => <Cost v={r.costUsd} /> },
              { header: "turns", cell: (r) => fmt.int(r.turns) },
              { header: "1-shot", cell: (r) => <OneShot v={r.oneShot} /> },
            ]}
            tooltip={(r) => (
              <>
                <b>{ACTIVITIES[r.key]?.label ?? r.key}</b>
                <div>{fmt.usd(r.costUsd)} · {fmt.int(r.turns)} turnos</div>
                {r.editTurns > 0 && <div className="muted">1-shot: turnos con ediciones sin fallos ni reediciones ({r.editTurns})</div>}
              </>
            )}
          />
        </TermPanel>

        <TermPanel title="By Model" color={C.model}>
          <TermTable
            rows={data.models}
            rowKey={(r) => r.key}
            value={(r) => r.costUsd}
            label={(r) => (
              <>
                {modelName(r.key)}
                {!r.hasPrice && <span className="term-badge">sin precio</span>}
              </>
            )}
            columns={[
              { header: "cost", cell: (r) => <Cost v={r.costUsd} /> },
              { header: "cache", cell: (r) => (r.cacheHit ? fmt.pct(r.cacheHit) : <span className="muted">-</span>) },
              { header: "calls", cell: calls },
              { header: "1-shot", cell: (r) => <OneShot v={oneShotByModel.get(r.key)} /> },
            ]}
            tooltip={(r) => (
              <>
                <b>{r.key}</b>
                <div>{fmt.usd(r.costUsd)} · {fmt.int(r.calls)} llamadas</div>
                <div className="muted">cache hit {fmt.pct(r.cacheHit)}</div>
              </>
            )}
          />
        </TermPanel>

        <TermPanel title="Core Tools" color={C.tools}>
          <TermTable rows={data.tools} rowKey={(r) => r.key} value={(r) => r.calls} label={(r) => r.label} columns={[{ header: "calls", cell: calls }]} tooltip={usesTooltip} />
        </TermPanel>

        <TermPanel title="Shell Commands" color={C.shell}>
          <TermTable rows={data.commands} rowKey={(r) => r.key} value={(r) => r.calls} label={(r) => r.label} columns={[{ header: "calls", cell: calls }]} tooltip={usesTooltip} />
        </TermPanel>

        <TermPanel title="Skills & Agents" color={C.skills}>
          <TermTable
            rows={data.skills}
            rowKey={(r) => r.key}
            value={(r) => r.calls}
            label={(r) => r.label}
            columns={[
              { header: "uses", cell: calls },
              { header: "cost", cell: (r) => <Cost v={r.costUsd} /> },
            ]}
            empty="ninguna skill ni subagente en este periodo"
            tooltip={(r) => (
              <>
                <b>{r.label}</b>
                <div>{fmt.int(r.calls)} usos</div>
                <div className="muted">coste de las respuestas que la invocaron: {fmt.usd(r.costUsd)}</div>
              </>
            )}
          />
        </TermPanel>

        <TermPanel title="MCP Servers" color={C.mcp}>
          <TermTable rows={data.mcp} rowKey={(r) => r.key} value={(r) => r.calls} label={(r) => r.label} columns={[{ header: "calls", cell: calls }]} empty="ningún servidor MCP en este periodo" tooltip={usesTooltip} />
        </TermPanel>

        <TermPanel title="Claude Agent Types" color={C.agents}>
          <TermTable
            rows={data.agentTypes}
            rowKey={(r) => r.key}
            value={(r) => r.costUsd}
            label={(r) => r.label}
            columns={[
              { header: "calls", cell: calls },
              { header: "cost", cell: (r) => <Cost v={r.costUsd} /> },
            ]}
            empty="ningún subagente en este periodo"
            tooltip={(r) => (
              <>
                <b>{r.label}</b>
                <div>{fmt.int(r.calls)} llamadas dentro de subagentes</div>
                <div className="muted">{fmt.usd(r.costUsd)}</div>
              </>
            )}
          />
        </TermPanel>
      </div>
    </div>
  );
}
