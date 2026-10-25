import { t } from "../lib/i18n";

/** Pestaña «Datos» de Ajustes: dónde se guarda la caché y cómo rehacerla. */
export function DataSettings({
  memoryOnly,
  rebuilding,
  onToggle,
  onRebuild,
  error,
}: {
  memoryOnly: boolean;
  rebuilding: boolean;
  onToggle: (memoryOnly: boolean) => void;
  onRebuild: () => void;
  error?: string | null;
}) {
  return (
    <section className="settings-section">
      <div className="settings-intro">
        <p className="intro-desc">{t("Dónde guarda AgentBoard lo que lee de los logs de tus agentes.")}</p>
      </div>
      <button type="button" className="chip-button" role="switch" aria-checked={memoryOnly} onClick={() => onToggle(!memoryOnly)}>
        <span className={`switch ${memoryOnly ? "on" : ""}`} aria-hidden />
        {t("Solo memoria")}
      </button>
      <p className="muted small">{t("No guarda la caché en disco; al abrir la app se vuelven a leer todos los logs.")}</p>
      <div className="export-row">
        <button type="button" className="chip" onClick={onRebuild} disabled={rebuilding}>
          {rebuilding ? t("Reconstruyendo…") : t("Reconstruir caché")}
        </button>
      </div>
      <p className="muted small">{t("Borra los datos guardados y vuelve a leer todos los logs.")}</p>
      {error && <p className="small critical">{t("No se pudo reconstruir la caché: {e}", { e: error })}</p>}
    </section>
  );
}
