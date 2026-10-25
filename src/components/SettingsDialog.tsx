import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type Language, type Settings, type Theme } from "../lib/api";
import { createRebuild, isDirty } from "../lib/cache";
import { LANGS, t } from "../lib/i18n";
import { Segmented } from "./Charts";
import { DataSettings } from "./DataSettings";

const THEMES: { value: Theme; label: string }[] = [
  { value: "system", label: "Sistema" },
  { value: "light", label: "Claro" },
  { value: "dark", label: "Oscuro" },
];

type Tab = "tema" | "idioma" | "datos" | "exportar";
const TABS: { id: Tab; label: string }[] = [
  { id: "tema", label: "Tema" },
  { id: "idioma", label: "Idioma" },
  { id: "datos", label: "Datos" },
  { id: "exportar", label: "Exportar" },
];

/** `ingest://done` del backend, con el oyente ya registrado al resolver. */
const listenDone = async (cb: () => void) => {
  const off = await listen("ingest://done", cb);
  return () => off();
};

/** Miniatura de la app en un tema: panel lateral, tarjetas y barras, con colores fijos. */
function Preview({ theme }: { theme: "light" | "dark" }) {
  const c =
    theme === "dark"
      ? { bg: "#14161a", side: "#191c21", card: "#1d2026", line: "#2b2f37", text: "#8a929d", accent: "#f2a33a", bar: "#5b8def" }
      : { bg: "#f3f3f1", side: "#f8f8f6", card: "#ffffff", line: "#e0dfda", text: "#a5a7ae", accent: "#20b0f0", bar: "#2a78d6" };
  return (
    <svg viewBox="0 0 160 100" width="100%" aria-hidden="true" style={{ display: "block", borderRadius: 8 }}>
      <rect width="160" height="100" fill={c.bg} />
      <rect width="42" height="100" fill={c.side} />
      <circle cx="21" cy="16" r="9" fill={c.accent} />
      {[34, 42, 50, 58].map((y) => (
        <rect key={y} x="9" y={y} width="24" height="3" rx="1.5" fill={c.text} />
      ))}
      <rect x="9" y="70" width="10" height="6" rx="3" fill={c.accent} />
      <rect x="22" y="70" width="12" height="6" rx="3" fill={c.line} />
      {[50, 86, 122].map((x) => (
        <rect key={x} x={x} y="8" width="30" height="16" rx="3" fill={c.card} stroke={c.line} />
      ))}
      {[50, 86, 122].map((x) => (
        <rect key={x} x={x + 4} y="16" width="14" height="3" rx="1.5" fill={c.accent} />
      ))}
      <rect x="50" y="31" width="102" height="61" rx="4" fill={c.card} stroke={c.line} />
      {[41, 50, 59, 68, 77].map((y, i) => (
        <g key={y}>
          <rect x="56" y={y} width="22" height="3" rx="1.5" fill={c.text} />
          <rect x="84" y={y - 1} width={60 - i * 11} height="5" rx="2.5" fill={c.bar} />
        </g>
      ))}
    </svg>
  );
}

function ThemeCard({ value, label, selected, onSelect }: { value: Theme; label: string; selected: boolean; onSelect: () => void }) {
  return (
    <button type="button" className={`theme-card ${selected ? "selected" : ""}`} onClick={onSelect} aria-pressed={selected}>
      <div className="theme-preview">
        {value === "system" ? (
          <div className="theme-split">
            <Preview theme="light" />
            <div className="theme-split-dark">
              <Preview theme="dark" />
            </div>
          </div>
        ) : (
          <Preview theme={value} />
        )}
      </div>
      <span className="theme-label">{label}</span>
    </button>
  );
}

/**
 * Ajustes: tema (con vista previa), idioma, datos (caché) y exportar. Lo elegido queda como
 * borrador hasta pulsar Aplicar; «Reconstruir caché» actúa al momento.
 */
export function SettingsDialog({
  settings,
  onSave,
  onClose,
  onExport,
}: {
  settings: Settings;
  onSave: (s: Settings) => Promise<void>;
  onClose: () => void;
  onExport: (format: "csv" | "json") => void;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const [tab, setTab] = useState<Tab>("tema");
  const [draft, setDraft] = useState<Settings>(settings);
  const dirty = isDirty(draft, settings);
  const [rebuilding, setRebuilding] = useState(false);
  const [rebuildError, setRebuildError] = useState<string | null>(null);
  const rebuild = useRef(createRebuild(api.rebuildCache, listenDone, (s) => setRebuilding(s === "running")));
  useEffect(() => {
    ref.current?.showModal();
    const r = rebuild.current;
    return () => r.dispose();
  }, []);
  const startRebuild = async () => {
    await rebuild.current.start();
    setRebuildError(rebuild.current.error());
  };
  const apply = async () => {
    await onSave(draft);
    onClose();
  };
  return (
    <dialog ref={ref} className="dialog dialog-settings" onClose={onClose}>
      <header className="dialog-head">
        <h2>{t("Ajustes")}</h2>
      </header>

      <div className="settings-tabs" role="tablist">
        {TABS.map((tb) => (
          <button
            key={tb.id}
            type="button"
            role="tab"
            aria-selected={tab === tb.id}
            className={`settings-tab ${tab === tb.id ? "active" : ""}`}
            onClick={() => setTab(tb.id)}
          >
            {t(tb.label)}
          </button>
        ))}
      </div>

      <div className="settings-body" role="tabpanel">
        {tab === "tema" && (
          <section className="settings-section">
            <div className="settings-intro">
              <p className="intro-desc">{t("Cómo se ve AgentBoard: claro, oscuro o siguiendo tu escritorio.")}</p>
            </div>
            <div className="theme-cards">
              {THEMES.map((o) => (
                <ThemeCard key={o.value} value={o.value} label={t(o.label)} selected={draft.theme === o.value} onSelect={() => setDraft({ ...draft, theme: o.value })} />
              ))}
            </div>
            <p className="muted small">{t('"Sistema" sigue el modo claro u oscuro de tu escritorio.')}</p>
          </section>
        )}

        {tab === "idioma" && (
          <section className="settings-section">
            <div className="settings-intro">
              <p className="intro-desc">{t("El idioma en el que se muestra toda la interfaz.")}</p>
            </div>
            <Segmented value={draft.language} options={LANGS.map((o) => ({ ...o, label: o.value === "system" ? t("Sistema") : o.label }))} onChange={(language: Language) => setDraft({ ...draft, language })} />
            <p className="muted small">{t('"Sistema" usa el idioma de tu escritorio (español, inglés, portugués o francés).')}</p>
          </section>
        )}

        {tab === "datos" && (
          <DataSettings
            memoryOnly={draft.memoryOnly}
            rebuilding={rebuilding}
            error={rebuildError}
            onToggle={(memoryOnly) => setDraft({ ...draft, memoryOnly })}
            onRebuild={startRebuild}
          />
        )}

        {tab === "exportar" && (
          <section className="settings-section">
            <div className="settings-intro">
              <p className="intro-desc">{t("Guarda tus datos en un archivo para usarlos fuera de la app.")}</p>
            </div>
            <div className="export-row">
              <button type="button" className="chip" onClick={() => onExport("csv")}>CSV</button>
              <button type="button" className="chip" onClick={() => onExport("json")}>JSON</button>
            </div>
            <p className="muted small">{t("Descarga los datos del periodo y los filtros actuales.")}</p>
          </section>
        )}
      </div>

      <div className="dialog-actions">
        <button type="button" className="button" onClick={onClose}>
          {t("Cancelar")}
        </button>
        <button type="button" className="button primary" onClick={apply} disabled={!dirty}>
          {t("Aplicar")}
        </button>
      </div>
    </dialog>
  );
}
