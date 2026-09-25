import { useEffect, useRef, useState } from "react";
import type { Language, Settings, Theme } from "../lib/api";
import { LANGS, t } from "../lib/i18n";
import { Segmented } from "./Charts";

const THEMES: { value: Theme; label: string }[] = [
  { value: "system", label: "Sistema" },
  { value: "light", label: "Claro" },
  { value: "dark", label: "Oscuro" },
];

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

/** Ajustes: tema (con vista previa) e idioma. Lo elegido queda como borrador hasta pulsar Aplicar. */
export function SettingsDialog({ settings, onSave, onClose }: { settings: Settings; onSave: (s: Settings) => Promise<void>; onClose: () => void }) {
  const ref = useRef<HTMLDialogElement>(null);
  const [draft, setDraft] = useState<Settings>(settings);
  const dirty = draft.theme !== settings.theme || draft.language !== settings.language;
  useEffect(() => {
    ref.current?.showModal();
  }, []);
  const apply = async () => {
    await onSave(draft);
    onClose();
  };
  return (
    <dialog ref={ref} className="dialog dialog-settings" onClose={onClose}>
      <header className="dialog-head">
        <h2>{t("Ajustes")}</h2>
      </header>

      <section className="settings-section">
        <h3>{t("Tema")}</h3>
        <div className="theme-cards">
          {THEMES.map((o) => (
            <ThemeCard key={o.value} value={o.value} label={t(o.label)} selected={draft.theme === o.value} onSelect={() => setDraft({ ...draft, theme: o.value })} />
          ))}
        </div>
        <p className="muted small">{t('"Sistema" sigue el modo claro u oscuro de tu escritorio.')}</p>
      </section>

      <section className="settings-section">
        <h3>{t("Idioma")}</h3>
        <Segmented value={draft.language} options={LANGS.map((o) => ({ ...o, label: o.value === "system" ? t("Sistema") : o.label }))} onChange={(language: Language) => setDraft({ ...draft, language })} />
        <p className="muted small">{t('"Sistema" usa el idioma de tu escritorio (español, inglés, portugués o francés).')}</p>
      </section>

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
