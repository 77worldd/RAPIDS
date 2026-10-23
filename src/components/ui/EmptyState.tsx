import type { ReactNode } from "react";
import { Button } from "./Button";

export interface EmptyAction {
  readonly label: string;
  readonly onClick: () => void;
  readonly variant?: "primary" | "secondary";
  readonly icon?: ReactNode;
}

interface EmptyStateProps {
  readonly icon: ReactNode;
  readonly title: string;
  readonly message: string;
  readonly primaryAction?: EmptyAction;
  readonly secondaryAction?: EmptyAction;
  readonly hint?: string;
}

export function EmptyState({
  icon,
  title,
  message,
  primaryAction,
  secondaryAction,
  hint,
}: EmptyStateProps) {
  return (
    <div className="flex h-full min-h-64 flex-col items-center justify-center px-6 py-12 text-center">
      <div className="relative mb-5 flex h-16 w-16 items-center justify-center rounded-2xl border border-line bg-panel text-emerald-300 shadow-[0_18px_40px_-26px_rgba(16,185,129,0.55)]">
        <div className="absolute inset-2 rounded-xl border border-emerald-400/10" />
        {icon}
      </div>
      <h2 className="text-[15px] font-semibold tracking-[-0.01em] text-zinc-100">
        {title}
      </h2>
      <p className="mt-2 max-w-md text-[12px] leading-relaxed text-zinc-500">
        {message}
      </p>
      {(primaryAction !== undefined || secondaryAction !== undefined) && (
        <div className="mt-5 flex flex-wrap items-center justify-center gap-2">
          {secondaryAction !== undefined && (
            <Button
              variant={secondaryAction.variant ?? "ghost"}
              onClick={secondaryAction.onClick}
            >
              {secondaryAction.icon}
              {secondaryAction.label}
            </Button>
          )}
          {primaryAction !== undefined && (
            <Button
              variant={primaryAction.variant ?? "primary"}
              onClick={primaryAction.onClick}
            >
              {primaryAction.icon}
              {primaryAction.label}
            </Button>
          )}
        </div>
      )}
      {hint !== undefined && (
        <p className="mt-4 text-[10.5px] text-zinc-600">{hint}</p>
      )}
    </div>
  );
}
