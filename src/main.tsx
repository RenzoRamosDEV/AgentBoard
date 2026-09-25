import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";

async function start() {
  // En `npm run dev` desde un navegador (fuera de Tauri) se usan datos de ejemplo.
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
    (await import("./dev/mock")).installMocks();
  }
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}

start();
