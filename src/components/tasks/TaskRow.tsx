import { ExternalLink, Pause, Play, RotateCcw, X } from "lucide-react";
import { categoryFor } from "../../config/categories";
import { cn } from "../../lib/cn";
import {
  formatBytes,
  formatEta,
  formatSpeed,
  progressFraction,
} from "../../lib/format";
import type { Task, TagId } from "../../types";
import type { TaskActions } from "../../state/useTaskActions";
import { StatusPill } from "./StatusPill";
import { TaskActionMenu } from "./TaskActionMenu";

const CATEGORY_TONE: Record<TagId, string> = {
  movies: "bg-violet-500/10 text-violet-300",
  music: "bg-sky-500/10 text-sky-300",
  images: "bg-rose-500/10 text-rose-300",
  documents: "bg-amber-500/10 text-amber-300",
  archives: "bg-orange-500/10 text-orange-300",
  apps: "bg-emerald-500/10 text-emerald-300",
  other: "bg-zinc-500/10 text-zinc-300",
};

function extensionOf(fileName: string): string {
  const dot = fileName.lastIndexOf(".");
  if (dot <= 0 || dot === fileName.length - 1) return "FILE";
  return fileName.slice(dot + 1).toUpperCase();
}

function statusWord(task: Task): string {
  switch (task.status) {
    case "downloading":
      return "Transferring";
    case "queued":
      return "Waiting for a slot";
    case "scheduled":
      return "Scheduled";
    case "paused":
      return "Paused";
    case "completed":
      return "Complete";
    case "error":
      return "Failed";
  }
}

function progressWidth(fraction: number | null): string {
  return fraction === null ? "0%" : `${Math.round(fraction * 100)}%`;
}

function actionLabel(task: Task): string {
  if (task.status === "downloading") return "Pause";
  if (task.status === "paused") return "Resume";
  if (task.status === "error") return "Retry";
  if (task.status === "completed") return "Open file";
  return "Cancel";
}

function RowAction({
  task,
  actions,
}: {
  readonly task: Task;
  readonly actions: TaskActions;
}) {
  const label = actionLabel(task);
  const icon =
    task.status === "downloading" ? (
      <Pause size={14} strokeWidth={1.9} />
    ) : task.status === "paused" ? (
      <Play size={14} strokeWidth={1.9} />
    ) : task.status === "error" ? (
      <RotateCcw size={14} strokeWidth={1.9} />
    ) : task.status === "completed" ? (
      <ExternalLink size={14} strokeWidth={1.9} />
    ) : (
      <X size={14} strokeWidth={1.9} />
    );

  function activate(): void {
    if (task.status === "downloading") actions.pause(task);
    else if (task.status === "paused") actions.resume(task);
    else if (task.status === "error") actions.retry(task);
    else if (task.status === "completed") actions.openFile(task);
    else actions.cancel(task);
  }

  return (
    <button
      type="button"
      onClick={(event) => {
        event.stopPropagation();
        activate();
      }}
      aria-label={`${label} ${task.fileName}`}
      title={label}
      className={cn(
        "inline-flex h-7 w-7 items-center justify-center rounded-md transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-emerald-500/65",
        task.status === "completed"
          ? "text-zinc-500 hover:bg-emerald-500/10 hover:text-emerald-300"
          : "text-zinc-400 hover:bg-white/[0.06] hover:text-zinc-100",
      )}
    >
      {icon}
    </button>
  );
}

export function TaskRow({
  task,
  selected,
  onToggle,
  actions,
}: {
  readonly task: Task;
  readonly selected: boolean;
  readonly onToggle: (id: number) => void;
  readonly actions: TaskActions;
}) {
  const category = categoryFor(task.category);
  const Icon = category.icon;
  const extension = extensionOf(task.fileName);
  const fraction = progressFraction(task);
  const percent =
    fraction === null ? null : Math.min(100, Math.max(0, Math.round(fraction * 100)));
  const unknownSize = task.totalBytes === null;
  const unknownProgress = fraction === null && task.status === "downloading";
  const size = unknownSize
    ? task.downloaded > 0
      ? formatBytes(task.downloaded)
      : "Unknown"
    : formatBytes(task.totalBytes);
  const downloadedLabel = unknownSize
    ? `${formatBytes(task.downloaded)} received`
    : `${formatBytes(task.downloaded)} of ${formatBytes(task.totalBytes)}`;

  return (
    <div
      role="row"
      aria-selected={selected}
      onClick={(event) => {
        if (
          event.target instanceof Element &&
          event.target.closest("button, input, a, select, label") !== null
        ) {
          return;
        }
        onToggle(task.id);
      }}
      className={cn(
        "group flex cursor-default items-center gap-2 border-b border-line/55 px-3 py-2.5 transition-colors duration-150 min-[1180px]:gap-3 min-[1180px]:px-4",
        selected
          ? "bg-emerald-500/[0.045]"
          : task.status === "error"
            ? "bg-red-500/[0.022] hover:bg-red-500/[0.04]"
            : "hover:bg-white/[0.025]",
      )}
    >
      <div className="flex w-5 shrink-0 items-center justify-center">
        <input
          type="checkbox"
          checked={selected}
          onChange={() => onToggle(task.id)}
          onClick={(event) => event.stopPropagation()}
          aria-label={`Select ${task.fileName}`}
          className="h-3.5 w-3.5 cursor-pointer rounded border-zinc-600 bg-transparent accent-emerald-500"
        />
      </div>

      <div className="flex min-w-0 flex-1 items-center gap-2.5">
        <span
          className={cn(
            "flex h-8 w-8 shrink-0 items-center justify-center rounded-lg",
            CATEGORY_TONE[task.category],
          )}
        >
          <Icon size={15} strokeWidth={1.8} />
        </span>
        <div className="min-w-0 flex-1">
          <p
            className="truncate text-[12px] font-medium text-zinc-200 group-hover:text-zinc-100"
            title={task.fileName}
          >
            {task.fileName}
          </p>
          <p className="mt-0.5 flex min-w-0 items-center gap-1 text-[10px] text-zinc-500">
            <span className="shrink-0 font-medium tracking-wide text-zinc-500">
              {extension}
            </span>
            <span className="text-zinc-700">·</span>
            <span className="shrink-0">{category.label}</span>
            <span className="text-zinc-700">·</span>
            <span className="truncate" title={task.destPath}>
              {task.destPath}
            </span>
          </p>
          {task.status === "error" && task.errorMessage !== null && (
            <p
              className="mt-0.5 truncate text-[10px] text-red-300/90"
              title={task.errorMessage}
            >
              {task.errorMessage}
            </p>
          )}
        </div>
      </div>

      <div className="w-16 shrink-0 text-right min-[1180px]:w-[4.5rem]">
        <p className="truncate text-[11px] tabular-nums text-zinc-300" title={size}>
          {size}
        </p>
        <p className="mt-0.5 text-[9.5px] text-zinc-600">
          {!unknownSize
            ? "total"
            : task.downloaded > 0
              ? "received"
              : task.status === "downloading" ||
                  task.status === "queued" ||
                  task.status === "scheduled"
                ? "pending"
                : "unreported"}
        </p>
      </div>

      <div className="w-[7rem] shrink-0 min-[1180px]:w-40">
        <div className="mb-1 flex items-center justify-between gap-2">
          <span className="truncate text-[10px] text-zinc-400">
            {statusWord(task)}
          </span>
          <span className="shrink-0 text-[9.5px] tabular-nums text-zinc-600">
            {percent === null ? "" : `${percent}%`}
          </span>
        </div>
        <div className="relative h-1.5 overflow-hidden rounded-full bg-white/[0.06]">
          <div
            className={cn(
              "h-full rounded-full transition-[width] duration-300 ease-out",
              unknownProgress && "rapids-progress-unknown w-1/3",
              task.status === "error"
                ? "bg-red-400/80"
                : task.status === "paused"
                  ? "bg-amber-400/80"
                  : task.status === "completed"
                    ? "bg-teal-400"
                    : task.status === "queued" || task.status === "scheduled"
                      ? "bg-sky-400/70"
                      : "bg-emerald-400",
            )}
            style={unknownProgress ? undefined : { width: progressWidth(fraction) }}
          />
        </div>
        <p className="mt-1 truncate text-[9.5px] tabular-nums text-zinc-600">
          {downloadedLabel}
        </p>
      </div>

      <div className="w-[3.75rem] shrink-0 text-right min-[1180px]:w-[4.2rem]">
        <p className="truncate text-[11px] tabular-nums text-zinc-300">
          {task.status === "downloading" && task.speedBps > 0
            ? formatSpeed(task.speedBps)
            : "—"}
        </p>
      </div>

      <div className="w-[3.25rem] shrink-0 text-right min-[1180px]:w-14">
        <p className="truncate text-[11px] tabular-nums text-zinc-300">
          {task.status === "downloading" ? formatEta(task.etaSeconds) : "—"}
        </p>
      </div>

      <div className="w-24 shrink-0 min-[1180px]:w-28">
        <StatusPill status={task.status} />
      </div>

      <div className="flex w-[3.6rem] shrink-0 items-center justify-end gap-0.5">
        <RowAction task={task} actions={actions} />
        <TaskActionMenu task={task} actions={actions} />
      </div>
    </div>
  );
}
