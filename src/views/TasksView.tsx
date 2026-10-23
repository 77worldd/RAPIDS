import {
  AlertTriangle,
  ArrowDownToLine,
  ArrowUpDown,
  ChevronDown,
  CircleCheckBig,
  CircleX,
  Clock,
  Download,
  Pause,
  Play,
  Plus,
  RefreshCw,
  RotateCcw,
  SearchX,
  Trash2,
  X,
} from "lucide-react";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { categoryFor } from "../config/categories";
import { normalizeClipboardUrl } from "../lib/fileMeta";
import {
  cancelTask,
  deleteTask,
  describeError,
  pauseTask,
  resumeTask,
  retryTask,
} from "../lib/ipc";
import { readClipboardText } from "../lib/native";
import {
  readWorkspacePrefs,
  writeWorkspacePrefs,
  type SortKey,
} from "../lib/preferences";
import {
  selectVisibleTasks,
  sortTasks,
} from "../state/selectors";
import {
  useRefreshTasks,
  useStoreDispatch,
  useStoreState,
} from "../state/StoreProvider";
import { useAddDownload } from "../state/AddDownloadProvider";
import type { Task, ViewId } from "../types";
import { TaskList } from "../components/tasks/TaskList";
import { Button } from "../components/ui/Button";
import { EmptyState, type EmptyAction } from "../components/ui/EmptyState";
import { IconButton } from "../components/ui/IconButton";

const SORT_OPTIONS: readonly { value: SortKey; label: string }[] = [
  { value: "newest", label: "Newest first" },
  { value: "oldest", label: "Oldest first" },
  { value: "name", label: "File name" },
  { value: "size", label: "Largest first" },
  { value: "progress", label: "Most complete" },
  { value: "status", label: "Status" },
];

function describeEmpty(viewId: ViewId, query: string): {
  readonly icon: ReactNode;
  readonly title: string;
  readonly message: string;
} {
  if (query.trim() !== "") {
    return {
      icon: <SearchX size={24} strokeWidth={1.6} />,
      title: "No matching downloads",
      message: `Nothing in this view matches “${query.trim()}”. Try another file name, host, or destination.`,
    };
  }

  switch (viewId) {
    case "all":
      return {
        icon: <ArrowDownToLine size={24} strokeWidth={1.6} />,
        title: "Your download queue is ready",
        message:
          "Paste a direct HTTP or HTTPS file link, or drop one anywhere in this window. RAPIDS checks the name, type, and destination before it queues the transfer.",
      };
    case "downloading":
      return {
        icon: <Download size={24} strokeWidth={1.6} />,
        title: "Nothing is downloading",
        message:
          "Add a direct file link or start a paused download. Transfers appear here while bytes are moving.",
      };
    case "queued":
      return {
        icon: <Clock size={24} strokeWidth={1.6} />,
        title: "The queue is clear",
        message:
          "New downloads wait here when every parallel slot is busy or when a start window is closed.",
      };
    case "completed":
      return {
        icon: <CircleCheckBig size={24} strokeWidth={1.6} />,
        title: "No completed downloads yet",
        message:
          "Finished files collect here. Open a file or reveal its folder from the row actions.",
      };
    case "paused":
      return {
        icon: <Pause size={24} strokeWidth={1.6} />,
        title: "No paused downloads",
        message:
          "Pause a running transfer to hold it here. Resume it whenever you are ready to continue.",
      };
    case "failed":
      return {
        icon: <CircleX size={24} strokeWidth={1.6} />,
        title: "No failed downloads",
        message:
          "A transfer that exhausts its retries appears here with the error it reported.",
      };
    default: {
      const category = categoryFor(viewId);
      const Icon = category.icon;
      return {
        icon: <Icon size={24} strokeWidth={1.6} />,
        title: `No ${category.label.toLowerCase()} downloads`,
        message: `${category.description} land in this category. You can change the category for any new download.`,
      };
    }
  }
}

export function TasksView() {
  const { tasks, viewId, query, status, notice, queuePaused } = useStoreState();
  const dispatch = useStoreDispatch();
  const refresh = useRefreshTasks();
  const { openAdd } = useAddDownload();
  const [sort, setSort] = useState<SortKey>(() => readWorkspacePrefs().sort);
  const [selected, setSelected] = useState<ReadonlySet<number>>(() => new Set());
  const [bulkBusy, setBulkBusy] = useState(false);

  useEffect(() => {
    writeWorkspacePrefs({ sort });
  }, [sort]);

  const visible = useMemo(
    () => selectVisibleTasks(tasks, viewId, query),
    [query, tasks, viewId],
  );
  const ordered = useMemo(() => sortTasks(visible, sort), [sort, visible]);
  const selectedTasks = useMemo(
    () => ordered.filter((task) => selected.has(task.id)),
    [ordered, selected],
  );
  const selectedCount = selectedTasks.length;

  const pausable = selectedTasks.filter((task) => task.status === "downloading");
  const resumable = selectedTasks.filter((task) => task.status === "paused");
  const retryable = selectedTasks.filter((task) => task.status === "error");

  function toggle(id: number): void {
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function toggleAll(select: boolean): void {
    setSelected(select ? new Set(ordered.map((task) => task.id)) : new Set());
  }

  async function runBulk(
    targets: readonly Task[],
    label: string,
    operation: (task: Task) => Promise<void>,
  ): Promise<void> {
    if (bulkBusy || targets.length === 0) return;
    setBulkBusy(true);
    const results = await Promise.allSettled(targets.map(operation));
    const failures = results.flatMap((result) =>
      result.status === "rejected" ? [result.reason as unknown] : [],
    );
    setBulkBusy(false);
    if (failures.length > 0) {
      dispatch({
        type: "notice/set",
        message: `${label}: ${failures.length} of ${targets.length} actions failed. ${describeError(failures[0])}`,
      });
    }
    setSelected(new Set());
    refresh();
  }

  function removeSelected(): void {
    if (
      !window.confirm(
        `Remove ${selectedTasks.length} download${selectedTasks.length === 1 ? "" : "s"} from the list? Files stay on disk.`,
      )
    ) {
      return;
    }
    void runBulk(selectedTasks, "Remove", async (task) => {
      if (
        task.status === "downloading" ||
        task.status === "queued" ||
        task.status === "scheduled"
      ) {
        await cancelTask(task.id);
      } else {
        await deleteTask(task.id, false);
      }
    });
  }

  async function pasteFromClipboard(): Promise<void> {
    try {
      const url = normalizeClipboardUrl(await readClipboardText());
      if (url === null) {
        dispatch({
          type: "notice/set",
          message: "The clipboard does not contain an HTTP or HTTPS download link.",
        });
        return;
      }
      openAdd(url);
    } catch (cause) {
      dispatch({ type: "notice/set", message: describeError(cause) });
    }
  }

  if (status === "loading") {
    return (
      <div className="flex h-full flex-col">
        <div className="flex h-11 shrink-0 items-center border-b border-line/70 px-4">
          <div className="h-3 w-28 animate-pulse rounded bg-zinc-800" />
        </div>
        <div className="min-h-0 flex-1 overflow-hidden">
          {[0, 1, 2, 3, 4, 5].map((row) => (
            <div
              key={row}
              className="flex h-[4.1rem] items-center gap-3 border-b border-line/55 px-4"
            >
              <div className="h-3.5 w-3.5 animate-pulse rounded bg-zinc-800" />
              <div className="h-8 w-8 animate-pulse rounded-lg bg-zinc-800" />
              <div className="flex-1 space-y-1.5">
                <div className="h-3 w-2/5 animate-pulse rounded bg-zinc-800" />
                <div className="h-2 w-3/5 animate-pulse rounded bg-zinc-800/70" />
              </div>
              <div className="hidden h-3 w-16 animate-pulse rounded bg-zinc-800 min-[1040px]:block" />
              <div className="h-1.5 w-32 animate-pulse rounded bg-zinc-800" />
              <div className="h-6 w-20 animate-pulse rounded-full bg-zinc-800" />
            </div>
          ))}
        </div>
      </div>
    );
  }

  const empty = describeEmpty(viewId, query);
  const emptyActions: { primary: EmptyAction; secondary?: EmptyAction } =
    query.trim() !== ""
      ? {
          primary: {
            label: "Clear search",
            onClick: () => dispatch({ type: "search/set", query: "" }),
            variant: "secondary",
          },
        }
      : {
          primary: {
            label: "Add download",
            onClick: () => openAdd(),
            icon: <Plus size={13} strokeWidth={2} />,
          },
          secondary: {
            label: "Paste from clipboard",
            onClick: () => void pasteFromClipboard(),
          },
        };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-11 shrink-0 flex-wrap items-center gap-2 border-b border-line/70 px-4">
        <p className="text-[11px] text-zinc-500">
          <span className="tabular-nums text-zinc-300">{ordered.length}</span>{" "}
          {ordered.length === 1 ? "download" : "downloads"}
          {query.trim() !== "" ? " found" : " in view"}
        </p>
        {query.trim() !== "" && (
          <span className="inline-flex max-w-[18rem] items-center gap-1 rounded-full border border-line bg-panel px-2 py-0.5 text-[10.5px] text-zinc-400">
            <span className="truncate">“{query.trim()}”</span>
            <button
              type="button"
              onClick={() => dispatch({ type: "search/set", query: "" })}
              aria-label="Clear search"
              className="inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-full text-zinc-500 hover:bg-white/[0.07] hover:text-zinc-200"
            >
              <X size={10} strokeWidth={2} />
            </button>
          </span>
        )}
        {queuePaused && (
          <span className="inline-flex items-center gap-1 rounded-full border border-amber-500/20 bg-amber-500/[0.06] px-2 py-0.5 text-[10px] text-amber-300">
            <Pause size={9} strokeWidth={2.2} /> Queue paused
          </span>
        )}
        <div className="ml-auto flex items-center gap-2">
          <label className="relative inline-flex items-center">
            <ArrowUpDown
              size={12}
              strokeWidth={1.8}
              className="pointer-events-none absolute left-2.5 text-zinc-500"
            />
            <span className="sr-only">Sort downloads</span>
            <select
              value={sort}
              onChange={(event) => setSort(event.target.value as SortKey)}
              className="h-7 appearance-none rounded-md border border-line bg-canvas/55 pl-7 pr-7 text-[11px] text-zinc-300 outline-none transition-colors focus:border-emerald-500/55 focus:ring-1 focus:ring-emerald-500/20"
            >
              {SORT_OPTIONS.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
            <ChevronDown
              size={12}
              strokeWidth={1.8}
              className="pointer-events-none absolute right-2 text-zinc-500"
            />
          </label>
          <IconButton label="Refresh downloads" onClick={refresh}>
            <RefreshCw size={13} strokeWidth={1.8} />
          </IconButton>
        </div>
      </div>

      {notice !== null && status !== "failed" && (
        <div
          role="alert"
          className="flex shrink-0 items-start gap-2 border-b border-red-900/50 bg-red-950/25 px-4 py-2 text-[11px] leading-relaxed text-red-200"
        >
          <AlertTriangle size={13} className="mt-0.5 shrink-0" strokeWidth={1.8} />
          <p className="min-w-0 flex-1">{notice}</p>
          <button
            type="button"
            onClick={() => dispatch({ type: "notice/clear" })}
            aria-label="Dismiss message"
            className="inline-flex h-4 w-4 shrink-0 items-center justify-center rounded text-red-300/70 hover:bg-red-500/10 hover:text-red-100"
          >
            <X size={11} strokeWidth={2} />
          </button>
        </div>
      )}

      {selectedCount > 0 && (
        <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-emerald-500/20 bg-emerald-500/[0.055] px-4 py-2">
          <span className="text-[11px] font-medium text-emerald-200">
            {selectedCount} selected
          </span>
          <div className="ml-auto flex flex-wrap items-center gap-1.5">
            <Button
              size="sm"
              variant="secondary"
              disabled={bulkBusy || pausable.length === 0}
              onClick={() =>
                void runBulk(pausable, "Pause", (task) => pauseTask(task.id))
              }
            >
              <Pause size={12} strokeWidth={2} /> Pause
            </Button>
            <Button
              size="sm"
              variant="secondary"
              disabled={bulkBusy || resumable.length === 0}
              onClick={() =>
                void runBulk(resumable, "Resume", (task) => resumeTask(task.id))
              }
            >
              <Play size={12} strokeWidth={2} /> Resume
            </Button>
            <Button
              size="sm"
              variant="secondary"
              disabled={bulkBusy || retryable.length === 0}
              onClick={() =>
                void runBulk(retryable, "Retry", (task) => retryTask(task.id))
              }
            >
              <RotateCcw size={12} strokeWidth={2} /> Retry
            </Button>
            <Button
              size="sm"
              variant="ghost"
              disabled={bulkBusy}
              onClick={removeSelected}
            >
              <Trash2 size={12} strokeWidth={1.8} /> Remove
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => setSelected(new Set())}
              disabled={bulkBusy}
            >
              Clear
            </Button>
          </div>
        </div>
      )}

      {status === "failed" ? (
        <div className="min-h-0 flex-1">
          <EmptyState
            icon={<AlertTriangle size={24} strokeWidth={1.6} />}
            title="The queue could not be loaded"
            message={notice ?? "The download service did not answer. Check that the desktop host is running, then try again."}
            primaryAction={{ label: "Try again", onClick: refresh, icon: <RefreshCw size={13} /> }}
          />
        </div>
      ) : ordered.length === 0 ? (
        <div className="min-h-0 flex-1">
          <EmptyState
            icon={empty.icon}
            title={empty.title}
            message={empty.message}
            primaryAction={emptyActions.primary}
            {...(emptyActions.secondary !== undefined
              ? { secondaryAction: emptyActions.secondary }
              : {})}
            hint={
              viewId === "queued" && queuePaused
                ? "The queue is paused. Start it from the toolbar when you are ready."
                : "Direct file links begin with http:// or https://. For media pages, use Inspect a media page."
            }
          />
        </div>
      ) : (
        <div className="min-h-0 flex-1">
          <TaskList
            tasks={ordered}
            selected={selected}
            onToggle={toggle}
            onToggleAll={toggleAll}
          />
        </div>
      )}
    </div>
  );
}
