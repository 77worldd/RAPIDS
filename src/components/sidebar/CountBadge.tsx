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
          ? "bg-emerald-400/15 text-emerald-300"
          : count > 0
            ? "bg-white/[0.055] text-zinc-400"
            : "bg-transparent text-zinc-600",
      )}
    >
      {count}
    </span>
  );
}
