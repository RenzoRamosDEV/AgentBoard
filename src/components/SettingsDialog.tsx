import { useEffect, useRef } from "react";
import type { Language, Settings, Theme } from "../lib/api";
import { LANGS, t } from "../lib/i18n";
import { Segmented } from "./Charts";

const THEMES: { value: Theme; label: string }[] = [
  { value: "system", label: "Sistema" },
  { value: "light", label: "Claro" },
  { value: "dark", label: "Oscuro" },
];

/** Ajustes: tema e idioma. Se aplican y se guardan al elegirlos. */
export function SettingsDialog({ settings, onSave, onClose }: { settings: Settings; onSave: (s: Settings) => Promise<void>; onClose: () => void }) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    ref.current?.showModal();
  }, []);
  return (
    <dialog ref={ref} className="dialog" onClose={onClose}>
      <h2>{t("Ajustes")}</h2>
      <div className="field">
        <span>{t("Tema")}</span>
        <Segmented value={settings.theme} options={THEMES.map((o) => ({ ...o, label: t(o.label) }))} onChange={(theme) => onSave({ ...settings, theme })} />
        <p className="muted small">{t('"Sistema" sigue el modo claro u oscuro de tu escritorio.')}</p>
      </div>
      <div className="field">
        <span>{t("Idioma")}</span>
        <Segmented value={settings.language} options={LANGS.map((o) => ({ ...o, label: o.value === "system" ? t("Sistema") : o.label }))} onChange={(language: Language) => onSave({ ...settings, language })} />
        <p className="muted small">{t('"Sistema" usa el idioma de tu escritorio (español, inglés, portugués o francés).')}</p>
      </div>
      <div className="dialog-actions">
        <button type="button" className="button primary" onClick={onClose}>
          {t("Cerrar")}
        </button>
      </div>
    </dialog>
  );
}
