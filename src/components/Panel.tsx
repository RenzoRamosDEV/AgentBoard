import type { ReactNode } from "react";

export function Panel({
  title,
  question,
  actions,
  className = "",
  children,
}: {
  title: string;
  question?: string;
  actions?: ReactNode;
  className?: string;
  children: ReactNode;
}) {
  return (
    <section className={`panel ${className}`}>
      <header>
        <div>
          <h2>{title}</h2>
          {question && <p className="muted">{question}</p>}
        </div>
        {actions}
      </header>
      {children}
    </section>
  );
}

export const Empty = ({ children = "Sin datos en este periodo" }: { children?: ReactNode }) => (
  <p className="muted empty-panel">{children}</p>
);
