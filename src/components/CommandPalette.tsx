import { useEffect, useMemo, useRef, useState } from "react";
import { api, type Filter, type ProjectRow, type SessionRow } from "../lib/api";
import { fmt } from "../lib/format";
import { filterKey, useQuery } from "../lib/query";

const NO_SESSIONS: SessionRow[] = [];
import { t } from "../lib/i18n";
import { SECTIONS, type SectionId } from "../lib/sections";
import { SearchIcon, SectionIcon } from "./Icons";

export interface PaletteItem {
  id: string;
  group: string;
  label: string;
  hint?: string;
  icon?: SectionId;
  run: () => void;
}

/** Elementos que casan con la búsqueda: todas sus palabras aparecen en la etiqueta o la pista. */
export function filterItems(items: PaletteItem[], q: string): PaletteItem[] {
  const words = q.trim().toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return items;
  return items.filter((i) => {
    const text = `${i.label} ${i.hint ?? ""} ${i.group}`.toLowerCase();
    return words.every((w) => text.includes(w));
  });
}

/** Buscador rápido (Ctrl+K): apartados, proyectos y sesiones recientes. */
export function CommandPalette({
  filter,
  projects,
  onClose,
  goSection,
  onlyProject,
  openSession,
  openSettings,
}: {
  filter: Filter;
  projects: ProjectRow[];
  onClose: () => void;
  goSection: (s: SectionId) => void;
  onlyProject: (id: number) => void;
  openSession: (id: string) => void;
  openSettings: () => void;
}) {
  const [q, setQ] = useState("");
  const [active, setActive] = useState(0);
  const recent = useQuery("list_sessions", `palette|${filterKey(filter)}`, () => api.sessions(filter, { limit: 30 }));
  const sessions: SessionRow[] = recent.data?.rows ?? NO_SESSIONS;
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLUListElement>(null);

  useEffect(() => input.current?.focus(), []);

  const items = useMemo<PaletteItem[]>(() => {
    const sectionItems = SECTIONS.map((s) => ({ id: `s:${s.id}`, group: t("Apartados"), label: t(s.title), hint: t(s.question), icon: s.id, run: () => goSection(s.id) }));
    const settings = { id: "s:settings", group: t("Apartados"), label: t("Ajustes"), hint: t("Tema, idioma y exportar"), run: openSettings };
    const projectItems = projects.map((p) => ({ id: `p:${p.id}`, group: t("Proyectos"), label: p.name, hint: `${fmt.usd(p.costUsd)} · ${p.cwd}`, run: () => onlyProject(p.id) }));
    const sessionItems = sessions.map((s) => ({
      id: `x:${s.id}`,
      group: t("Sesiones recientes"),
      label: `${s.project ?? t("(sin proyecto)")}${s.branch ? ` · ${s.branch}` : ""}`,
      hint: `${fmt.dateTime(s.startedAt)} · ${s.agentName} · ${fmt.usd(s.costUsd)}`,
      run: () => openSession(s.id),
    }));
    return [...sectionItems, settings, ...projectItems, ...sessionItems];
  }, [projects, sessions, goSection, onlyProject, openSession, openSettings]);

  const shown = filterItems(items, q).slice(0, 40);
  const choose = (i: PaletteItem | undefined) => {
    if (!i) return;
    i.run();
    onClose();
  };

  useEffect(() => setActive(0), [q]);
  useEffect(() => {
    list.current?.querySelector(`[data-index="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }, [active]);

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((a) => Math.min(shown.length - 1, a + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => Math.max(0, a - 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      choose(shown[active]);
    } else if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  };

  let lastGroup = "";
  return (
    <div className="palette-backdrop" onMouseDown={onClose}>
      <div className="palette" role="dialog" aria-label={t("Buscar")} onMouseDown={(e) => e.stopPropagation()} onKeyDown={onKey}>
        <label className="palette-input">
          <SearchIcon />
          <input
            ref={input}
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder={t("Apartado, proyecto o sesión…")}
            aria-label={t("Buscar")}
            role="combobox"
            aria-expanded="true"
            aria-controls="palette-list"
            aria-activedescendant={shown[active] ? `pal-${active}` : undefined}
          />
          <kbd>Esc</kbd>
        </label>
        <ul className="palette-list" id="palette-list" role="listbox" ref={list}>
          {shown.map((i, n) => {
            const head = i.group !== lastGroup ? i.group : null;
            lastGroup = i.group;
            return (
              <li key={i.id} role="presentation">
                {head && <div className="palette-group">{head}</div>}
                <div
                  id={`pal-${n}`}
                  role="option"
                  aria-selected={n === active}
                  data-index={n}
                  className={`palette-item ${n === active ? "active" : ""}`}
                  onMouseEnter={() => setActive(n)}
                  onClick={() => choose(i)}
                >
                  {i.icon ? <SectionIcon id={i.icon} /> : <span className="palette-dot" />}
                  <span className="palette-label">{i.label}</span>
                  {i.hint && <span className="palette-hint">{i.hint}</span>}
                </div>
              </li>
            );
          })}
          {!shown.length && <li className="palette-empty muted">{t("Nada coincide con «{q}»", { q })}</li>}
        </ul>
      </div>
    </div>
  );
}
