import { viewLabel } from "../config/navigation";
import { TaskList } from "../components/tasks/TaskList";
import { EmptyState } from "../components/ui/EmptyState";
import { Skeleton } from "../components/ui/Skeleton";
import { selectVisibleTasks } from "../state/selectors";
import {
  useRefreshTasks,
  useStoreDispatch,
  useStoreState,
} from "../state/StoreProvider";
import type { ViewId } from "../types";

function emptyMessageFor(viewId: ViewId): string {
  switch (viewId) {
    case "all":
      return "No downloads yet.";
    case "running":
      return "Nothing is downloading right now.";
    case "paused":
      return "Nothing is paused.";
    case "completed":
      return "Completed downloads collect here.";
    case "unfinished":
      return "No downloads are left unfinished.";
    case "waiting":
      return "Nothing is waiting to start.";
    case "finished":
      return "Finished downloads collect here.";
    default:
      return `No ${viewLabel(viewId)} downloads yet.`;
  }
}

function TaskListSkeleton() {
  return (
    <div
      role="status"
      aria-busy="true"
      aria-label="Loading downloads"
      className="space-y-2 p-4"
    >
      {Array.from({ length: 6 }, (_, index) => (
        <div
          key={index}
          className="flex h-11 items-center gap-6 rounded-md border border-zinc-900 bg-zinc-900/40 px-4"
        >
          <Skeleton className="h-3.5 w-64" />
          <Skeleton className="h-3.5 w-20" />
          <Skeleton className="h-3.5 w-24" />
          <Skeleton className="ml-auto h-3.5 w-16" />
        </div>
      ))}
    </div>
  );
}

function Notice() {
  const { notice } = useStoreState();
  const dispatch = useStoreDispatch();
  if (notice === null) return null;

  return (
    <div role="alert" className="flex items-center gap-3 border-b border-red-900/60 bg-red-950/40 px-4 py-2 text-[12px] text-red-200">
      <span className="min-w-0 flex-1 truncate" title={notice}>
        {notice}
      </span>
      <button
        type="button"
        className="rounded px-1.5 text-[11px] text-red-300 hover:bg-red-900/40 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-red-300"
        onClick={() => {
          dispatch({ type: "notice/clear" });
        }}
      >
        Dismiss
      </button>
    </div>
  );
}

export function TasksView() {
  const { status, tasks, viewId, query } = useStoreState();
  const dispatch = useStoreDispatch();
  const refresh = useRefreshTasks();

  if (status === "loading") {
    return (
      <div className="flex h-full min-h-0 flex-col">
        <Notice />
        <TaskListSkeleton />
      </div>
    );
  }

  if (status === "failed") {
    return (
      <div className="flex h-full min-h-0 flex-col">
        <Notice />
        <EmptyState
          message="The download service is not answering, so the list cannot be shown."
          action={{ label: "Try again", onClick: refresh }}
        />
      </div>
    );
  }

  const visible = selectVisibleTasks(tasks, viewId, query);
  return (
    <div className="flex h-full min-h-0 flex-col">
      <Notice />
      {visible.length === 0 ? (
        query.trim() !== "" ? (
          <EmptyState
            message={`No downloads match “${query.trim()}”.`}
            action={{
              label: "Clear search",
              onClick: () => {
                dispatch({ type: "search/set", query: "" });
              },
            }}
          />
        ) : (
          <EmptyState message={emptyMessageFor(viewId)} action={null} />
        )
      ) : (
        <TaskList tasks={visible} />
      )}
    </div>
  );
}
