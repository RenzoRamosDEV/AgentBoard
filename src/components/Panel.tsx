import type { ReactNode } from "react";
import { t } from "../lib/i18n";

/** Tarjeta de un apartado: título, pregunta que responde y enlace a su vista ampliada. */
export function Panel({
  id,
  title,
  question,
  onOpen,
  wide = false,
  children,
}: {
  id?: string;
  title: string;
  question?: string;
  onOpen?: () => void;
  wide?: boolean;
  children: ReactNode;
}) {
  return (
    <section id={id} className={`panel ${wide ? "panel-wide" : ""}`}>
      <header className="panel-head">
        <div className="panel-title">
          <h2>{title}</h2>
          {question && <span className="muted">{question}</span>}
        </div>
      </header>
      {children}
      {onOpen && (
        <footer className="panel-foot">
          <button className="link" onClick={onOpen}>
            {t("Ver más ›")}
          </button>
        </footer>
      )}
    </section>
  );
}

/** Tabla a la izquierda, gráfico a la derecha. */
export const Split = ({ table, chart, chartWidth = 220 }: { table: ReactNode; chart: ReactNode; chartWidth?: number }) => (
  <div className="split">
    <div className="split-table">{table}</div>
    <div className="split-chart" style={{ flexBasis: chartWidth }}>
      {chart}
    </div>
  </div>
);

export const ChartTitle = ({ children }: { children: ReactNode }) => <div className="chart-title">{children}</div>;

export const Empty = ({ children }: { children?: ReactNode }) => <p className="empty">{children ?? t("Sin datos en este periodo")}</p>;
