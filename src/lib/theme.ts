import type { Theme } from "./api";

const media = window.matchMedia("(prefers-color-scheme: light)");

/** Tema efectivo: el del sistema cuando la preferencia es "system". */
export const resolveTheme = (t: Theme): "light" | "dark" => (t === "system" ? (media.matches ? "light" : "dark") : t);

/** Aplica el tema al documento; con "system" sigue los cambios del sistema hasta que se llame de nuevo. */
export function applyTheme(t: Theme): () => void {
  const set = () => {
    document.documentElement.dataset.theme = resolveTheme(t);
  };
  set();
  try {
    localStorage.setItem("theme", t);
  } catch {
    // sin almacenamiento: no pasa nada
  }
  if (t !== "system") return () => {};
  media.addEventListener("change", set);
  return () => media.removeEventListener("change", set);
}

/** Preferencia recordada en este equipo, para pintar bien antes de leer los ajustes. */
export function storedTheme(): Theme {
  try {
    const t = localStorage.getItem("theme");
    if (t === "light" || t === "dark" || t === "system") return t;
  } catch {
    // ignorar
  }
  return "system";
}
