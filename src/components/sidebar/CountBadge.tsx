import { cn } from "../../lib/cn";

interface CountBadgeProps {
  readonly count: number;
  readonly active: boolean;
}

export function CountBadge({ count, active }: CountBadgeProps) {
  return (
    <span
      className={cn(
        "inline-flex min-w-[1.35rem] items-center justify-center rounded-full px-1.5 py-px text-[10px] leading-none tabular-nums transition-colors",
        active && count > 0
          ? "bg-ink/15 text-ink"
          : count > 0
            ? "bg-ink/[0.055] text-muted"
            : "bg-transparent text-faint",
      )}
    >
      {count}
    </span>
  );
}
