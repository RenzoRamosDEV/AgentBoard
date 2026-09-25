import { createContext, useContext, useState, type ReactNode } from "react";

interface Tip {
  x: number;
  y: number;
  content: ReactNode;
}

const Ctx = createContext<(tip: Tip | null) => void>(() => {});

/** Tooltip flotante compartido por todas las gráficas. */
export function TooltipProvider({ children }: { children: ReactNode }) {
  const [tip, setTip] = useState<Tip | null>(null);
  return (
    <Ctx.Provider value={setTip}>
      {children}
      {tip && (
        <div
          className="tooltip"
          role="tooltip"
          style={{
            left: Math.min(tip.x + 14, window.innerWidth - 240),
            top: tip.y + 14,
          }}
        >
          {tip.content}
        </div>
      )}
    </Ctx.Provider>
  );
}

export const useTooltip = () => useContext(Ctx);
