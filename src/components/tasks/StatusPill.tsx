import type { TaskStatus } from "../../types";

const PRESENTATION: Record<TaskStatus, { label: string; color: string }> = {
  queued: { label: "Queued", color: "bg-sky-500/10 text-sky-300" },
  scheduled: { label: "Scheduled", color: "bg-violet-500/10 text-violet-300" },
  downloading: { label: "Downloading", color: "bg-emerald-500/10 text-emerald-300" },
  paused: { label: "Paused", color: "bg-amber-500/10 text-amber-300" },
  completed: { label: "Completed", color: "bg-emerald-500/10 text-emerald-300" },
  error: { label: "Failed", color: "bg-red-500/10 text-red-300" },
};

export function StatusPill({ status }: { readonly status: TaskStatus }) {
  const { label, color } = PRESENTATION[status];
  return (
    <span
      className={`inline-flex shrink-0 items-center rounded-full px-2 py-1 text-[11px] font-medium leading-none ${color}`}
    >
      {label}
    </span>
  );
}
