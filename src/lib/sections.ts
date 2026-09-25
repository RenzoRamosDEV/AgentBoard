/** Apartados del panel izquierdo, en el orden en que se muestran. */
export type SectionId =
  | "overview"
  | "daily"
  | "project"
  | "activity"
  | "model"
  | "tools"
  | "shell"
  | "skills"
  | "mcp"
  | "agents";

export interface Section {
  id: SectionId;
  title: string;
  question: string;
}

export const SECTIONS: Section[] = [
  { id: "overview", title: "Resumen", question: "Todo de un vistazo" },
  { id: "daily", title: "Daily Activity", question: "¿Cuánto gasto cada día?" },
  { id: "project", title: "By Project", question: "¿Cuánto costó cada proyecto?" },
  { id: "activity", title: "By Activity", question: "¿En qué se va el gasto?" },
  { id: "model", title: "By Model", question: "¿Uso el modelo adecuado?" },
  { id: "tools", title: "Tools", question: "¿Qué herramientas usa y dónde fallan?" },
  { id: "shell", title: "Shell Commands", question: "¿Qué comandos ejecuta?" },
  { id: "skills", title: "Skills & Agents", question: "¿Qué skills y subagentes invoco?" },
  { id: "mcp", title: "MCP Servers", question: "¿Qué servidores MCP uso?" },
  { id: "agents", title: "Claude Agent Types", question: "¿Cuánto cuestan los subagentes?" },
];

export const sectionOf = (id: SectionId) => SECTIONS.find((s) => s.id === id)!;
