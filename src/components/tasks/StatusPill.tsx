import { CalendarClock, Check, Clock3, Loader2, Pause, X } from "lucide-react";
import type { TaskStatus } from "../../types";
import { cn } from "../../lib/cn";

const PRESENTATION: Record<
  TaskStatus,
  { label: string; tone: string; icon: typeof Check; spin?: boolean }
> = {
  queued: {
    label: "Queued",
    tone: "bg-ink/[0.055] text-muted",
    icon: Clock3,
  },
  scheduled: {
    label: "Scheduled",
    tone: "bg-ink/[0.035] text-muted",
    icon: CalendarClock,
  },
  downloading: {
    label: "Downloading",
    tone: "bg-ink/10 text-ink",
    icon: Loader2,
    spin: true,
  },
  paused: {
    label: "Paused",
    tone: "bg-ink/[0.07] text-ink-soft",
    icon: Pause,
  },
  completed: {
    label: "Completed",
    tone: "bg-ink text-canvas",
    icon: Check,
  },
  error: {
    label: "Failed",
    tone: "border border-ink/25 bg-transparent text-ink",
    icon: X,
  },
};

export function StatusPill({ status }: { readonly status: TaskStatus }) {
  const { label, tone, icon: Icon, spin } = PRESENTATION[status];
  return (
    <span
      className={cn(
        "inline-flex shrink-0 items-center gap-1.5 rounded-full px-2 py-1 text-[10.5px] font-medium leading-none",
        tone,
      )}
    >
      <Icon
        size={11}
        strokeWidth={2}
        aria-hidden="true"
        className={cn(spin && "animate-spin")}
      />
      {label}
    </span>
  );
}
