import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource/sora/600.css";
import "@fontsource/sora/700.css";
import "@fontsource/ibm-plex-sans/400.css";
import "@fontsource/ibm-plex-sans/500.css";
import "@fontsource/ibm-plex-sans/600.css";
import "@fontsource/ibm-plex-mono/500.css";
import App from "./App";
import { TrayPanel } from "./views/TrayPanel";
import "./styles.css";

async function start() {
  const params = new URLSearchParams(window.location.search);
  const probe = import.meta.env.DEV && params.get("probe") === "1";
  // En `npm run dev` (o en la build `perf` de medición) desde un navegador, fuera de Tauri, se usan datos de ejemplo.
  const mocked = import.meta.env.DEV || import.meta.env.VITE_MOCK === "1";
  if (mocked && !("__TAURI_INTERNALS__" in window)) {
    const mock = await import("./dev/mock");
    mock.installMocks();
    // `?probe=1` pinta una chapa que delata cualquier desborde (para capturas responsive).
    if (probe) mock.installOverflowProbe();
  }
  // La ventana `panel` de la bandeja carga la misma app con `?panel=1`.
  const isPanel = params.get("panel") === "1";
  // `?perf=1` en la build `perf`: el arnés mide la app y deja los resultados en el documento.
  const harness = import.meta.env.VITE_MOCK === "1" && params.get("perf") === "1" ? await import("./dev/perfHarness") : null;
  const app = isPanel ? <TrayPanel /> : <App />;
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>{harness ? harness.profiled(app) : app}</React.StrictMode>,
  );
  if (harness) await harness.runHarness();
  // Con la sonda, el evento `load` espera al primer pintado: las capturas headless salen ya con datos.
  if (probe) await new Promise((r) => setTimeout(r, 1500));
}

// `await` de nivel de módulo: retrasa `load` hasta que `start()` termina (lo usan las capturas).
await start();
