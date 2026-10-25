import { useEffect, useRef, useState, type ReactNode } from "react";
import { ChevronIcon } from "./Icons";

/** Botón con un panel desplegable debajo; se cierra al pulsar fuera o con Escape. */
export function Popover({
  label,
  children,
  align = "left",
  className = "",
  ariaLabel,
  chevron = true,
  perf,
}: {
  label: ReactNode;
  children: ReactNode | ((close: () => void) => ReactNode);
  align?: "left" | "right";
  className?: string;
  ariaLabel?: string;
  chevron?: boolean;
  /** Atributo `data-perf` del botón (sin efecto visual; lo usa el arnés de medición). */
  perf?: string;
}) {
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLDivElement>(null);
  const close = () => setOpen(false);

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (wrap.current && !wrap.current.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div className="popover-wrap" ref={wrap}>
      <button type="button" data-perf={perf} className={`chip-button ${className}`} aria-expanded={open} aria-haspopup="dialog" aria-label={ariaLabel} onClick={() => setOpen((v) => !v)}>
        {label}
        {chevron && <ChevronIcon />}
      </button>
      {open && (
        <div className={`popover ${align === "right" ? "align-right" : ""}`} role="dialog" aria-label={ariaLabel}>
          {typeof children === "function" ? children(close) : children}
        </div>
      )}
    </div>
  );
}
