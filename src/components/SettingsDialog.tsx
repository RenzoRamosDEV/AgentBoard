import { useEffect, useRef } from "react";
import type { Settings, Theme } from "../lib/api";
import { Segmented } from "./Charts";

const THEMES: { value: Theme; label: string }[] = [
  { value: "system", label: "Sistema" },
  { value: "light", label: "Claro" },
  { value: "dark", label: "Oscuro" },
];

/** Ajustes: por ahora, el tema de la interfaz. Se aplica y se guarda al elegirlo. */
export function SettingsDialog({ settings, onSave, onClose }: { settings: Settings; onSave: (s: Settings) => Promise<void>; onClose: () => void }) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    ref.current?.showModal();
  }, []);
  return (
    <dialog ref={ref} className="dialog" onClose={onClose}>
      <h2>Ajustes</h2>
      <div className="field">
        <span>Tema</span>
        <Segmented value={settings.theme} options={THEMES} onChange={(theme) => onSave({ ...settings, theme })} />
        <p className="muted small">"Sistema" sigue el modo claro u oscuro de tu escritorio.</p>
      </div>
      <div className="dialog-actions">
        <button type="button" className="button primary" onClick={onClose}>
          Cerrar
        </button>
      </div>
    </dialog>
  );
}
