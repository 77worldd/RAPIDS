import { viewLabel } from "../config/navigation";
import { EmptyState } from "../components/ui/EmptyState";
import { Skeleton } from "../components/ui/Skeleton";
import { selectVisibleTasks } from "../state/selectors";
import { useStoreDispatch, useStoreState } from "../state/StoreProvider";
import type { ViewId } from "../types";

function emptyMessageFor(viewId: ViewId): string {
  switch (viewId) {
    case "all":
      return "No downloads yet — add a URL to start your first download.";
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
      return "Finished schedules collect here.";
    default:
      return `No ${viewLabel(viewId)} downloads yet.`;
  }
}

function TaskListSkeleton() {
  return (
    <div
      role="status"
      aria-busy="true"
      aria-label="Counting downloads"
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

export function TasksView() {
  const { status, tasks, viewId, query } = useStoreState();
  const dispatch = useStoreDispatch();

  if (status === "loading") {
    return <TaskListSkeleton />;
  }

  const visible = selectVisibleTasks(tasks, viewId, query);

  if (visible.length === 0) {
    if (query.trim() !== "") {
      return (
        <EmptyState
          message={`No downloads match “${query.trim()}”.`}
          action={{
            label: "Clear search",
            onClick: () => dispatch({ type: "search/set", query: "" }),
          }}
        />
      );
    }
    return <EmptyState message={emptyMessageFor(viewId)} action={null} />;
  }

  // Task rows render here once the engine feeds the live task list.
  return null;
}
