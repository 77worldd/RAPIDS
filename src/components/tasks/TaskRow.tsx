import {
  ArrowDown,
  ArrowUp,
  FileDown,
  Pause,
  Play,
  RotateCcw,
  Trash2,
  X,
} from "lucide-react";
import {
  formatBytes,
  formatEta,
  formatSpeed,
  progressFraction,
  shortHash,
} from "../../lib/format";
import type { Task } from "../../types";
import { IconButton } from "../ui/IconButton";
import { StatusPill } from "./StatusPill";

export interface TaskActions {
  readonly pause: (id: number) => void;
  readonly resume: (id: number) => void;
  readonly retry: (id: number) => void;
  readonly cancel: (id: number) => void;
  readonly remove: (id: number, removeFile: boolean) => void;
  readonly reorder: (id: number, priority: number) => void;
}

export function TaskRow({
  task,
  actions,
}: {
  readonly task: Task;
  readonly actions: TaskActions;
}) {
  const fraction = progressFraction(task);
  const size =
    task.totalBytes === null
      ? task.downloaded > 0
        ? `${formatBytes(task.downloaded)} transferred · size unknown`
        : "Size unknown"
      : `${formatBytes(task.downloaded)} / ${formatBytes(task.totalBytes)}`;
  const waiting =
    task.status === "queued" ||
    task.status === "scheduled" ||
    task.status === "paused";
  const finished = task.status === "completed" || task.status === "error";

  return (
    <li className="flex flex-wrap items-center gap-x-4 gap-y-2 px-4 py-3 transition-colors duration-150 ease-out hover:bg-zinc-900/50">
      <div className="min-w-[12rem] flex-[2]">
        <div className="flex min-w-0 items-center gap-2">
          <FileDown size={15} strokeWidth={1.75} className="shrink-0 text-zinc-500" />
          <span className="truncate text-[13px] font-medium text-zinc-100" title={task.fileName}>
            {task.fileName}
          </span>
          <span className="shrink-0 text-[11px] text-zinc-600">
            {task.category}
          </span>
          {waiting && (
            <span className="shrink-0 text-[11px] text-zinc-600">
              Priority {task.priority}
            </span>
          )}
        </div>
        <p className="mt-1 truncate pl-[23px] text-[11px] text-zinc-500" title={task.url}>
          {task.url}
        </p>
        <p className="mt-0.5 truncate pl-[23px] text-[11px] text-zinc-600" title={task.destPath}>
          {task.destPath}
        </p>
        {task.errorMessage !== null && (
          <p
            className="mt-1 truncate pl-[23px] text-[11px] text-red-300"
            title={task.errorMessage}
          >
            {task.errorMessage}
          </p>
        )}
        {task.status === "completed" && task.sha256 !== null && (
          <p className="mt-1 pl-[23px] text-[11px] text-emerald-400/80" title={task.sha256}>
            SHA-256 {shortHash(task.sha256)}
          </p>
        )}
      </div>

      <div className="w-40 shrink-0 text-[11px] tabular-nums text-zinc-400">
        <div className="flex items-center justify-between gap-2">
          <span className="truncate" title={size}>{size}</span>
          {fraction !== null && (
            <span className="shrink-0 text-zinc-500">
              {Math.round(fraction * 100)}%
            </span>
          )}
        </div>
        {fraction !== null && (
          <div
            role="progressbar"
            aria-label={`Download progress for ${task.fileName}`}
            aria-valuenow={Math.round(fraction * 100)}
            aria-valuemin={0}
            aria-valuemax={100}
            className="mt-1.5 h-1 w-full overflow-hidden rounded-full bg-zinc-800"
          >
            <div
              className="h-full rounded-full bg-emerald-500 transition-[width] duration-150 ease-out"
              style={{ width: `${fraction * 100}%` }}
            />
          </div>
        )}
        {task.status === "downloading" && (
          <div className="mt-1.5 flex justify-between text-zinc-500">
            <span>{formatSpeed(task.speedBps)}</span>
            <span>ETA {formatEta(task.etaSeconds)}</span>
          </div>
        )}
      </div>

      <StatusPill status={task.status} />

      <div className="ml-auto flex shrink-0 items-center gap-0.5">
        {waiting && (
          <>
            <IconButton
              label={`Raise priority (currently ${task.priority})`}
              onClick={() => {
                actions.reorder(task.id, task.priority + 1);
              }}
            >
              <ArrowUp size={14} strokeWidth={1.75} />
            </IconButton>
            <IconButton
              label={`Lower priority (currently ${task.priority})`}
              onClick={() => {
                actions.reorder(task.id, task.priority - 1);
              }}
            >
              <ArrowDown size={14} strokeWidth={1.75} />
            </IconButton>
          </>
        )}
        {task.status === "downloading" && (
          <IconButton
            label="Pause download"
            onClick={() => {
              actions.pause(task.id);
            }}
          >
            <Pause size={14} strokeWidth={1.75} />
          </IconButton>
        )}
        {task.status === "paused" && (
          <IconButton
            label="Resume download"
            onClick={() => {
              actions.resume(task.id);
            }}
          >
            <Play size={14} strokeWidth={1.75} />
          </IconButton>
        )}
        {task.status === "error" && (
          <IconButton
            label="Retry download"
            onClick={() => {
              actions.retry(task.id);
            }}
          >
            <RotateCcw size={14} strokeWidth={1.75} />
          </IconButton>
        )}
        {finished ? (
          <IconButton
            label={task.status === "completed" ? "Remove from list and keep file" : "Remove failed download"}
            onClick={() => {
              actions.remove(task.id, false);
            }}
          >
            <X size={14} strokeWidth={1.75} />
          </IconButton>
        ) : (
          <IconButton
            label="Cancel download"
            onClick={() => {
              actions.cancel(task.id);
            }}
          >
            <X size={14} strokeWidth={1.75} />
          </IconButton>
        )}
        {task.status === "completed" && (
          <IconButton
            label="Delete downloaded file"
            className="text-red-400 hover:text-red-300"
            onClick={() => {
              if (window.confirm(`Delete the downloaded file at ${task.destPath}?`)) {
                actions.remove(task.id, true);
              }
            }}
          >
            <Trash2 size={14} strokeWidth={1.75} />
          </IconButton>
        )}
      </div>
    </li>
  );
}
