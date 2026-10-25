import { useState } from "react";
import { api, type AgentRow, type BreakdownRow, type Filter, type PriceOverride, type PriceRow, type ScopedBudget, type Settings } from "../lib/api";
import { fmt } from "../lib/format";
import { filterKey, useQuery } from "../lib/query";
import { t } from "../lib/i18n";
import { monthStart, projectMonth } from "../lib/period";
import { DataTable, type Column } from "../components/DataTable";
import { Select } from "../components/Select";

/** Columnas de precio, en el orden de `PriceRow.prices`. */
const FIELDS = ["input", "output", "cacheRead", "cacheWrite", "cacheWrite1h"] as const;
const FIELD_LABELS = ["Entrada", "Salida", "Caché lect.", "Caché 5 min", "Caché 1 h"];

type Draft = Record<string, string[]>;

const SOURCES: Record<string, { label: string; className: string }> = {
  default: { label: "Por defecto", className: "src-default" },
  edited: { label: "Editado", className: "src-edited" },
  reported: { label: "Coste del agente", className: "src-reported" },
  missing: { label: "Falta precio", className: "src-missing" },
};

/** Texto del campo → número válido (≥ 0); vacío cuenta como 0; `null` si no es válido. */
export function parsePrice(v: string): number | null {
  const s = v.trim().replace(",", ".");
  if (s === "") return 0;
  const n = Number(s);
  return Number.isFinite(n) && n >= 0 ? n : null;
}

/** Aplica los borradores a los precios del usuario: los modelos editados se añaden o sustituyen. */
export function mergeOverrides(current: PriceOverride[], draft: Draft): PriceOverride[] | null {
  const out = new Map(current.map((p) => [p.model, p]));
  for (const [model, values] of Object.entries(draft)) {
    const nums = values.map(parsePrice);
    if (nums.some((n) => n == null)) return null;
    const [input, output, cacheRead, cacheWrite, cacheWrite1h] = nums as number[];
    out.set(model, { model, input, output, cacheRead, cacheWrite, cacheWrite1h });
  }
  return [...out.values()];
}

/**
 * Lee un JSON de precios: una lista `[{ model, input, output, cacheRead?, cacheWrite?, cacheWrite1h? }]`
 * o un objeto `{ "modelo": { input, output, … } }`. Devuelve borradores (texto) por modelo.
 */
export function parsePriceFile(text: string): Draft {
  const data = JSON.parse(text);
  const entries: [string, Record<string, unknown>][] = Array.isArray(data)
    ? data.map((x) => [String(x?.model ?? ""), x ?? {}])
    : Object.entries(data ?? {}).map(([m, x]) => [m, (x ?? {}) as Record<string, unknown>]);
  const out: Draft = {};
  for (const [model, x] of entries) {
    const m = model.trim().toLowerCase();
    if (!m) throw new Error("falta el nombre de un modelo");
    out[m] = FIELDS.map((f) => (x[f] == null ? "" : String(x[f])));
    if (out[m].some((v) => parsePrice(v) == null)) throw new Error(`precio inválido en ${m}`);
  }
  return out;
}

const show = (n: number | undefined) => (n == null ? "" : n === 0 ? "" : String(n));

function Progress({ label, spent, limit, hint }: { label: string; spent: number; limit: number; hint: string }) {
  const ratio = limit > 0 ? spent / limit : 0;
  const tone = ratio >= 1 ? "critical" : ratio >= 0.8 ? "warn" : "good";
  return (
    <div className="budget-progress">
      <div className="budget-progress-head">
        <span>{label}</span>
        <span className={`num ${tone}`}>{fmt.pct(ratio)}</span>
      </div>
      <div className="bar-track">
        <div className={`bar-fill fill-${tone}`} style={{ width: `${Math.min(100, ratio * 100)}%` }} />
      </div>
      <span className="muted small num">{hint}</span>
    </div>
  );
}

/** Apartado Precios y presupuestos. */
export function Pricing({
  filter,
  settings,
  onSave,
  agents,
}: {
  filter: Filter;
  settings: Settings;
  onSave: (s: Settings) => Promise<void>;
  agents: AgentRow[];
}) {
  const pricesQ = useQuery<PriceRow[]>("list_prices", filterKey(filter), () => api.prices(filter));
  const rows = pricesQ.data ?? null;
  const [draft, setDraft] = useState<Draft>({});
  const [showAll, setShowAll] = useState(false);
  const [newModel, setNewModel] = useState("");
  const [saving, setSaving] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  // Errores de guardar/importar: se muestran junto al formulario, sin sustituir el editor.
  const [opError, setOpError] = useState<string | null>(null);
  const [confirmReset, setConfirmReset] = useState(false);
  // Gasto del mes (todos los agentes y proyectos) para las barras de presupuesto.
  const now = new Date();
  const m: Filter = { from: monthStart(now).getTime() };
  const today: Filter = { from: new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime() };
  const monthQ = useQuery("pricing_month", `${m.from}|${today.from}`, async () => {
    const [sm, st, projects, ags] = await Promise.all([api.summary(m), api.summary(today), api.breakdown(m, "project"), api.breakdown(m, "agent")]);
    return { total: sm.costUsd, today: st.costUsd, projects, agents: ags };
  });
  const month = monthQ.data ?? null;
  const error = pricesQ.error ?? monthQ.error;

  if (error) return <div className="main error">{t("No se pudieron cargar los datos: {e}", { e: error })}</div>;
  if (!rows) return <div className="main muted">{t("Cargando…")}</div>;

  const missing = rows.filter((r) => r.source === "missing");
  const missingCalls = missing.reduce((a, r) => a + r.calls, 0);
  const draftModels = Object.keys(draft).filter((m) => !rows.some((r) => r.model === m));
  const visible = [
    ...draftModels.map((model): PriceRow => ({ model, prices: null, source: "missing", calls: 0, costUsd: 0 })),
    ...rows.filter((r) => showAll || r.calls > 0 || r.source !== "default" || draft[r.model]),
  ];
  const hidden = rows.length + draftModels.length - visible.length;
  const dirty = Object.keys(draft).length > 0;
  const invalid = Object.values(draft).some((v) => v.some((x) => parsePrice(x) == null));

  const valueOf = (r: PriceRow, i: number) => draft[r.model]?.[i] ?? show(r.prices?.[i]);
  const edit = (r: PriceRow, i: number, v: string) => {
    const base = draft[r.model] ?? FIELDS.map((_, j) => show(r.prices?.[j]));
    const next = [...base];
    next[i] = v;
    setDraft({ ...draft, [r.model]: next });
  };

  const save = async (next: Settings, message: string) => {
    setSaving(true);
    setOpError(null);
    try {
      await onSave(next);
      setNotice(message);
      setTimeout(() => setNotice(null), 2500);
      return true;
    } catch (e) {
      setOpError(t("No se pudo guardar: {e}", { e: String(e) }));
      return false;
    } finally {
      setSaving(false);
    }
  };
  const savePrices = async () => {
    const priceOverrides = mergeOverrides(settings.priceOverrides, draft);
    if (!priceOverrides) return;
    // El borrador solo se descarta cuando el guardado se confirma; si falla, sigue ahí para reintentar.
    if (await save({ ...settings, priceOverrides }, t("Precios guardados: los costes ya están recalculados."))) setDraft({});
  };
  const restore = (model: string) =>
    save({ ...settings, priceOverrides: settings.priceOverrides.filter((p) => p.model !== model) }, t("Precio restablecido."));
  const addModel = () => {
    const m = newModel.trim().toLowerCase();
    if (!m || draft[m] || rows.some((r) => r.model === m)) return;
    setDraft({ ...draft, [m]: FIELDS.map(() => "") });
    setNewModel("");
  };

  /** Lleva el foco al primer precio que falta. */
  const completeMissing = () => {
    const first = missing[0];
    if (first) document.getElementById(`price-${first.model}-0`)?.focus();
  };
  const restoreAll = async () => {
    if (await save({ ...settings, priceOverrides: [] }, t("Precios por defecto restaurados."))) setDraft({});
    setConfirmReset(false);
  };
  const importJson = async (file: File | undefined) => {
    if (!file) return;
    try {
      const parsed = parsePriceFile(await file.text());
      setDraft({ ...draft, ...parsed });
      setNotice(t("{n} precios importados: revísalos y guarda.", { n: Object.keys(parsed).length }));
    } catch (e) {
      setOpError(t("No se pudo importar el archivo: {e}", { e: String(e) }));
    }
  };

  const columns: Column<PriceRow>[] = [
    { header: t("Modelo"), cell: (r) => <span title={r.model}>{r.model}</span>, className: "mono", width: "minmax(150px, 1fr)" },
    ...FIELDS.map(
      (_, i): Column<PriceRow> => ({
        header: t(FIELD_LABELS[i]),
        cell: (r) => (
          <input
            id={`price-${r.model}-${i}`}
            className={`price-input ${parsePrice(valueOf(r, i)) == null ? "invalid" : ""} ${r.source === "missing" && i < 2 && !draft[r.model] ? "needs" : ""}`}
            inputMode="decimal"
            placeholder="–"
            aria-label={t("{campo} de {modelo}", { campo: t(FIELD_LABELS[i]), modelo: r.model })}
            value={valueOf(r, i)}
            onChange={(e) => edit(r, i, e.target.value)}
          />
        ),
        align: "right",
        width: "84px",
      }),
    ),
    {
      header: t("Origen"),
      cell: (r) => {
        const src = draft[r.model] ? { label: "Sin guardar", className: "src-edited" } : SOURCES[r.source];
        return <span className={`src-tag ${src.className}`}>{t(src.label)}</span>;
      },
      width: "118px",
    },
    { header: t("Llamadas"), cell: (r) => (r.calls ? fmt.int(r.calls) : "–"), align: "right", width: "72px", className: "secondary", sort: (r) => r.calls },
    {
      header: "",
      cell: (r) =>
        draft[r.model] ? (
          <button className="link" onClick={() => setDraft(Object.fromEntries(Object.entries(draft).filter(([m]) => m !== r.model)))}>
            {t("Descartar")}
          </button>
        ) : r.source === "edited" ? (
          <button className="link" onClick={() => restore(r.model)} disabled={saving}>
            {t("Restablecer")}
          </button>
        ) : null,
      width: "84px",
      align: "right",
    },
  ];

  // --- Presupuestos -----------------------------------------------------------------
  const factor = month && month.total > 0 ? projectMonth(month.total) / month.total : 1;
  const spentOf = (b: ScopedBudget) => (month ? ((b.kind === "project" ? month.projects : month.agents).find((r) => r.key === b.key)?.costUsd ?? 0) : 0);

  return (
    <div className="main">
      <header className="page-head">
        <h1>
          {t("Precios y presupuestos")} <span className="muted">· {t("se guardan en settings.json; el resto sigue en memoria")}</span>
        </h1>
      </header>
      {missing.length > 0 && (
        <div className="notice warn notice-row">
          <span>
          {t(missing.length === 1 ? "1 modelo sin precio ({m}) · {c} llamadas se cuentan a $0. Escribe su precio abajo y guarda." : "{n} modelos sin precio ({m}) · {c} llamadas se cuentan a $0. Escribe su precio abajo y guarda.", {
            n: missing.length,
            m: missing.map((r) => r.model).join(", "),
            c: fmt.int(missingCalls),
          })}
          </span>
          <button className="button primary" onClick={completeMissing}>
            {t("Completar precios")}
          </button>
        </div>
      )}
      {notice && <div className="notice good-notice">{notice}</div>}
      {opError && (
        <div className="notice warn notice-row">
          <span>{opError}</span>
          <button className="button" onClick={() => setOpError(null)}>
            {t("Cerrar")}
          </button>
        </div>
      )}
      <div className="grid-pricing">
        <section className="panel">
          <header className="panel-head">
            <div className="panel-title">
              <h2>{t("Precios por modelo")}</h2>
              <span className="muted">{t("USD por millón de tokens · un cambio recalcula todos los costes al momento")}</span>
            </div>
          </header>
          <div className="toolbar">
            <label className="search">
              <input placeholder={t("Nombre del modelo, p. ej. kimi-k2")} value={newModel} onChange={(e) => setNewModel(e.target.value)} onKeyDown={(e) => e.key === "Enter" && addModel()} aria-label={t("Modelo nuevo")} />
            </label>
            <button className="button" onClick={addModel} disabled={!newModel.trim()}>
              {t("+ Añadir modelo")}
            </button>
            <label className="button file-button">
              {t("Importar JSON")}
              <input type="file" accept="application/json,.json" onChange={(e) => (importJson(e.target.files?.[0]), (e.target.value = ""))} />
            </label>
            <div className="topbar-spacer" />
            <button className="button primary" onClick={savePrices} disabled={!dirty || invalid || saving}>
              {t("Guardar precios")}
            </button>
          </div>
          {invalid && <p className="error small">{t("Los precios deben ser números mayores o iguales que 0.")}</p>}
          <DataTable rows={visible} rowKey={(r) => r.model} columns={columns} collapse={false} rowClassName={(r) => (r.source === "missing" ? "row-missing" : "")} />
          <footer className="pricing-foot">
            {hidden > 0 ? (
              <button className="link" onClick={() => setShowAll(true)}>
                {t("Ver también los {n} modelos con precio por defecto que no has usado", { n: hidden })}
              </button>
            ) : (
              <span />
            )}
            {settings.priceOverrides.length > 0 &&
              (confirmReset ? (
                <span className="confirm">
                  {t("¿Quitar tus {n} precios?", { n: settings.priceOverrides.length })}
                  <button className="link danger" onClick={restoreAll} disabled={saving}>
                    {t("Sí, restaurar")}
                  </button>
                  <button className="link" onClick={() => setConfirmReset(false)}>
                    {t("Cancelar")}
                  </button>
                </span>
              ) : (
                <button className="link" onClick={() => setConfirmReset(true)}>
                  {t("Restaurar valores por defecto")}
                </button>
              ))}
          </footer>
        </section>

        <Budgets settings={settings} month={month} factor={factor} spentOf={spentOf} agents={agents} saving={saving} save={save} />
      </div>
    </div>
  );
}

function Budgets({
  settings,
  month,
  factor,
  spentOf,
  agents,
  saving,
  save,
}: {
  settings: Settings;
  month: { total: number; today: number; projects: BreakdownRow[]; agents: BreakdownRow[] } | null;
  factor: number;
  spentOf: (b: ScopedBudget) => number;
  agents: AgentRow[];
  saving: boolean;
  save: (s: Settings, message: string) => Promise<boolean>;
}) {
  const [monthly, setMonthly] = useState(settings.monthlyBudget?.toString() ?? "");
  const [daily, setDaily] = useState(settings.dailyBudget?.toString() ?? "");
  const [kind, setKind] = useState<"project" | "agent">("project");
  const [key, setKey] = useState("");
  const [amount, setAmount] = useState("");
  const num = (v: string) => (v.trim() === "" ? null : parsePrice(v));
  const validTop = num(monthly) !== null || monthly.trim() === "";
  const validDay = num(daily) !== null || daily.trim() === "";
  const dirtyTop = (settings.monthlyBudget?.toString() ?? "") !== monthly.trim() || (settings.dailyBudget?.toString() ?? "") !== daily.trim();

  const options = kind === "project"
    ? (month?.projects ?? []).map((r) => ({ key: r.key, label: r.label }))
    : agents.map((a) => ({ key: a.id, label: a.name }));
  const free = options.filter((o) => !settings.budgets.some((b) => b.kind === kind && b.key === o.key));
  const amountN = parsePrice(amount);

  const add = () => {
    const opt = free.find((o) => o.key === key) ?? free[0];
    if (!opt || !amountN) return;
    save({ ...settings, budgets: [...settings.budgets, { kind, key: opt.key, label: opt.label, monthly: amountN }] }, t("Presupuesto añadido."));
    setAmount("");
    setKey("");
  };
  const remove = (b: ScopedBudget) => save({ ...settings, budgets: settings.budgets.filter((x) => x !== b) }, t("Presupuesto quitado."));

  return (
    <section className="panel budgets">
      <header className="panel-head">
        <div className="panel-title">
          <h2>{t("Presupuestos")}</h2>
        </div>
      </header>
      <div className="budget-fields">
        <label className="field">
          <span>{t("Mensual (USD)")}</span>
          <input inputMode="decimal" placeholder={t("sin límite")} value={monthly} onChange={(e) => setMonthly(e.target.value)} className={validTop ? "" : "invalid"} />
        </label>
        <label className="field">
          <span>{t("Diario (USD)")}</span>
          <input inputMode="decimal" placeholder={t("sin límite")} value={daily} onChange={(e) => setDaily(e.target.value)} className={validDay ? "" : "invalid"} />
        </label>
      </div>
      <button
        className="button primary budget-save"
        disabled={!dirtyTop || !validTop || !validDay || saving}
        onClick={() => save({ ...settings, monthlyBudget: num(monthly), dailyBudget: num(daily) }, t("Presupuestos guardados."))}
      >
        {t("Guardar")}
      </button>
      {month && settings.monthlyBudget != null && settings.monthlyBudget > 0 && (
        <Progress
          label={t("Este mes")}
          spent={month.total * factor}
          limit={settings.monthlyBudget}
          hint={t("{s} gastado · proyección {p} de {b}", { s: fmt.usd(month.total), p: fmt.usd(month.total * factor), b: fmt.usd(settings.monthlyBudget) })}
        />
      )}
      {month && settings.dailyBudget != null && settings.dailyBudget > 0 && (
        <Progress label={t("Hoy")} spent={month.today} limit={settings.dailyBudget} hint={t("{s} de {b}", { s: fmt.usd(month.today), b: fmt.usd(settings.dailyBudget) })} />
      )}

      <div className="budget-scoped">
        <h3>{t("Por proyecto y agente")}</h3>
        {settings.budgets.map((b) => (
          <div key={`${b.kind}:${b.key}`} className="scoped-row">
            <Progress
              label={`${b.label} · ${t(b.kind === "project" ? "proyecto" : "agente")}`}
              spent={spentOf(b) * factor}
              limit={b.monthly}
              hint={t("{s} gastado · proyección {p} de {b}", { s: fmt.usd(spentOf(b)), p: fmt.usd(spentOf(b) * factor), b: fmt.usd(b.monthly) })}
            />
            <button className="link" onClick={() => remove(b)} disabled={saving} aria-label={t("Quitar el presupuesto de {n}", { n: b.label })}>
              {t("Quitar")}
            </button>
          </div>
        ))}
        {!settings.budgets.length && <p className="muted small">{t("Acota el gasto de un proyecto o de un agente concreto.")}</p>}
        <div className="scoped-add">
          <Select
            ariaLabel={t("Tipo")}
            value={kind}
            options={[
              { value: "project", label: t("Proyecto") },
              { value: "agent", label: t("Agente") },
            ]}
            onChange={(k: "project" | "agent") => (setKind(k), setKey(""))}
          />
          {free.length ? (
            <Select ariaLabel={t(kind === "project" ? "Proyecto" : "Agente")} value={key || free[0].key} options={free.map((o) => ({ value: o.key, label: o.label }))} onChange={setKey} />
          ) : (
            <span className="muted small">{t("Todos tienen ya presupuesto")}</span>
          )}
          <input inputMode="decimal" placeholder={t("USD/mes")} value={amount} onChange={(e) => setAmount(e.target.value)} aria-label={t("Importe mensual")} />
          <button className="button" onClick={add} disabled={!free.length || !amountN || saving}>
            {t("Añadir")}
          </button>
        </div>
      </div>
      <fieldset className="alert-prefs">
        <legend>{t("Avisos")}</legend>
        {(
          [
            ["alertAt80", "Notificar al llegar al 80 %"],
            ["alertAt100", "Notificar al superar el 100 %"],
            ["trayShowsToday", "Mostrar el gasto de hoy en la bandeja"],
          ] as const
        ).map(([k, label]) => (
          <label key={k} className="check">
            <input type="checkbox" checked={settings[k]} disabled={saving} onChange={(e) => save({ ...settings, [k]: e.target.checked }, t("Avisos guardados."))} />
            {t(label)}
          </label>
        ))}
      </fieldset>
      <p className="muted small">{t("Los mensuales comparan la proyección del mes; el diario, el gasto de hoy.")}</p>
    </section>
  );
}
