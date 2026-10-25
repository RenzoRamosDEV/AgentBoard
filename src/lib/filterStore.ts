/**
 * Filtro global (periodo, agentes y proyectos ocultos, comparación) fuera de React: cada
 * componente se suscribe solo a la parte que usa, así un cambio de filtro no vuelve a pintar
 * la app entera.
 */
import { useMemo, useSyncExternalStore } from "react";
import { api, type AgentRow, type Filter, type ProjectRow } from "./api";
import { periodRange, previousRange, type Period } from "./period";
import { filterKey, useGeneration, useQuery } from "./query";

export interface FilterState {
  period: Period;
  /** Se guardan los *ocultos*: un agente o proyecto nuevo aparece incluido por defecto. */
  hiddenAgents: ReadonlySet<string>;
  hiddenProjects: ReadonlySet<number>;
  compare: boolean;
}

export interface Derived {
  filter: Filter;
  key: string;
  /** Mismo filtro de agentes y proyectos sobre el periodo anterior (no hay con «Todo» ni sin comparar). */
  prevFilter: Filter | null;
  /** Nombre del proyecto si solo hay uno visible. */
  singleProject: string | null;
}

type Storage = { getItem(k: string): string | null; setItem(k: string, v: string): void };

const COMPARE_KEY = "agentboard.compare";

const toggled = <T,>(set: ReadonlySet<T>, v: T) => {
  const next = new Set(set);
  if (next.has(v)) next.delete(v);
  else next.add(v);
  return next;
};

/** Ids de los agentes visibles; sin ocultos, ningún filtro (`undefined`). */
export function visibleAgents(hidden: ReadonlySet<string>, agents: AgentRow[]): string[] | undefined {
  return hidden.size ? agents.filter((a) => !hidden.has(a.id)).map((a) => a.id) : undefined;
}

export function createFilterStore(storage: Storage | null) {
  let compare = true;
  try {
    compare = storage?.getItem(COMPARE_KEY) !== "0";
  } catch {
    // sin almacenamiento: comparación activa
  }
  let state: FilterState = { period: { kind: "30d" }, hiddenAgents: new Set(), hiddenProjects: new Set(), compare };
  const listeners = new Set<() => void>();
  let last: Derived | null = null;

  const set = (patch: Partial<FilterState>) => {
    state = { ...state, ...patch };
    listeners.forEach((l) => l());
  };

  const subscribe = (l: () => void) => {
    listeners.add(l);
    return () => void listeners.delete(l);
  };

  return {
    get: () => state,
    subscribe,
    /** Avisa solo cuando cambia lo que devuelve `sel`. */
    watch<T>(sel: (s: FilterState) => T, l: () => void) {
      let prev = sel(state);
      return subscribe(() => {
        const next = sel(state);
        if (Object.is(next, prev)) return;
        prev = next;
        l();
      });
    },
    setPeriod: (period: Period) => set({ period }),
    toggleAgent: (id: string) => set({ hiddenAgents: toggled(state.hiddenAgents, id) }),
    toggleProject: (id: number) => set({ hiddenProjects: toggled(state.hiddenProjects, id) }),
    onlyProject: (id: number | null, all: number[]) => set({ hiddenProjects: new Set(id === null ? [] : all.filter((p) => p !== id)) }),
    setCompare(v: boolean) {
      set({ compare: v });
      try {
        storage?.setItem(COMPARE_KEY, v ? "1" : "0");
      } catch {
        // sin almacenamiento: vale para esta sesión
      }
    },
    /** El filtro de las consultas; el mismo objeto mientras no cambie su clave. */
    derive(range: { from?: number; to?: number }, agents: AgentRow[], projects: ProjectRow[], now = new Date()): Derived {
      const { hiddenAgents, hiddenProjects } = state;
      const filter: Filter = {
        ...range,
        agents: visibleAgents(hiddenAgents, agents),
        projects: hiddenProjects.size ? projects.filter((p) => !hiddenProjects.has(p.id)).map((p) => p.id) : undefined,
      };
      const visible = projects.filter((p) => !hiddenProjects.has(p.id));
      const singleProject = hiddenProjects.size && visible.length === 1 ? visible[0].name : null;
      const prev = state.compare ? previousRange(state.period, now) : null;
      const prevFilter = prev ? { ...filter, ...prev } : null;
      const key = filterKey(filter);
      if (last && last.key === key && last.singleProject === singleProject && (last.prevFilter ? filterKey(last.prevFilter) : null) === (prevFilter ? filterKey(prevFilter) : null))
        return last;
      last = { filter, key, prevFilter, singleProject };
      return last;
    },
    /** El último filtro derivado (para exportar sin suscribirse). */
    current: () => last,
  };
}

export type FilterStore = ReturnType<typeof createFilterStore>;

const browserStorage = (): Storage | null => {
  try {
    return localStorage;
  } catch {
    return null;
  }
};

export const filterStore = createFilterStore(typeof localStorage === "undefined" ? null : browserStorage());

/** Una parte del estado del filtro; el componente solo se vuelve a pintar si cambia. */
export function useFilterState<T>(sel: (s: FilterState) => T): T {
  const read = () => sel(filterStore.get());
  return useSyncExternalStore(filterStore.subscribe, read, read);
}

const NO_AGENTS: AgentRow[] = [];
const NO_PROJECTS: ProjectRow[] = [];

/** Rango del periodo; con cada aviso de datos se recalcula (los periodos relativos avanzan al cambiar de día). */
export function useRange() {
  const period = useFilterState((s) => s.period);
  const generation = useGeneration();
  return useMemo(() => periodRange(period), [period, generation]);
}

/** Listas de agentes (del periodo) y proyectos (del periodo y los agentes visibles), de la caché. */
export function useLists() {
  const range = useRange();
  const hiddenAgents = useFilterState((s) => s.hiddenAgents);
  const agentsQ = useQuery("list_agents", filterKey(range), () => api.agents(range));
  const agents = agentsQ.data ?? NO_AGENTS;
  const scope: Filter = { ...range, agents: visibleAgents(hiddenAgents, agents) };
  const projectsQ = useQuery("list_projects", filterKey(scope), () => api.projects(scope));
  return { range, agents, projects: projectsQ.data ?? NO_PROJECTS, error: agentsQ.error ?? projectsQ.error };
}

/** El filtro activo y lo que de él depende; el mismo objeto mientras nada cambie. */
export function useFilter(): Derived & { agents: AgentRow[]; projects: ProjectRow[]; period: Period } {
  const { range, agents, projects } = useLists();
  const period = useFilterState((s) => s.period);
  useFilterState((s) => s.hiddenProjects);
  useFilterState((s) => s.compare);
  const derived = filterStore.derive(range, agents, projects);
  return useMemo(() => ({ ...derived, agents, projects, period }), [derived, agents, projects, period]);
}
