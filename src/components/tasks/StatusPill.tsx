import type { TaskStatus } from "../../types";
import { cn } from "../../lib/cn";

const PRESENTATION: Record<
  TaskStatus,
  { label: string; color: string; dot: string }
> = {
  queued: {
    label: "Queued",
    color: "bg-sky-500/10 text-sky-300",
    dot: "bg-sky-400",
  },
  scheduled: {
    label: "Scheduled",
    color: "bg-violet-500/10 text-violet-300",
    dot: "bg-violet-400",
  },
  downloading: {
    label: "Downloading",
    color: "bg-accent/10 text-accent-bright",
    dot: "bg-accent-bright",
  },
  paused: {
    label: "Paused",
    color: "bg-amber-500/10 text-amber-300",
    dot: "bg-amber-400",
  },
  completed: {
    label: "Completed",
    color: "bg-accent-deep/20 text-accent-bright",
    dot: "bg-accent-deep",
  },
  error: {
    label: "Failed",
    color: "bg-red-500/10 text-red-300",
    dot: "bg-red-400",
  },
};

export function StatusPill({ status }: { readonly status: TaskStatus }) {
  const { label, color, dot } = PRESENTATION[status];
  return (
    <span
      className={cn(
        "inline-flex shrink-0 items-center gap-1.5 rounded-full px-2 py-1 text-[10.5px] font-medium leading-none",
        color,
      )}
    >
      <span
        className={cn(
          "h-1.5 w-1.5 rounded-full",
          dot,
          status === "downloading" && "animate-pulse",
        )}
      />
      {label}
    </span>
  );
}
