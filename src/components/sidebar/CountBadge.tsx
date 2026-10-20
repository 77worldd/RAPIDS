import { cn } from "../../lib/cn";

interface CountBadgeProps {
  readonly count: number;
  readonly active: boolean;
}

export function CountBadge({ count, active }: CountBadgeProps) {
  return (
    <span
      className={cn(
        "inline-flex min-w-[1.4rem] items-center justify-center rounded-full px-1.5 py-px text-[11px] leading-none tabular-nums",
        active && count > 0
          ? "bg-emerald-500/15 text-emerald-400"
          : "bg-zinc-800 text-zinc-400",
      )}
    >
      {count}
    </span>
  );
}
