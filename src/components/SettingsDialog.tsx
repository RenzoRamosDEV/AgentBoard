import { useEffect, useRef, useState } from "react";
import type { Settings } from "../lib/api";

export function SettingsDialog({
  settings,
  onSave,
  onClose,
}: {
  settings: Settings;
  onSave: (s: Settings) => Promise<void>;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const [budget, setBudget] = useState(settings.monthlyBudget?.toString() ?? "");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    ref.current?.showModal();
  }, []);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    const raw = budget.trim().replace(",", ".");
    const value = raw === "" ? null : Number(raw);
    if (value !== null && (!Number.isFinite(value) || value < 0)) {
      setError("Introduce un número mayor o igual que 0, o déjalo vacío.");
      return;
    }
    try {
      await onSave({ ...settings, monthlyBudget: value });
      onClose();
    } catch (err) {
      setError(String(err));
    }
  };

  return (
    <dialog ref={ref} className="dialog" onClose={onClose}>
      <form onSubmit={submit}>
        <h2>Ajustes</h2>
        <label className="field">
          <span>Presupuesto mensual (USD)</span>
          <input inputMode="decimal" placeholder="Sin presupuesto" value={budget} onChange={(e) => setBudget(e.target.value)} autoFocus />
        </label>
        {error && <p className="error">{error}</p>}
        <div className="dialog-actions">
          <button type="button" className="button" onClick={onClose}>
            Cancelar
          </button>
          <button type="submit" className="button primary">
            Guardar
          </button>
        </div>
      </form>
    </dialog>
  );
}
