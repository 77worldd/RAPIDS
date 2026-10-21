import type { Task, ViewId } from "../types";

function taskInView(task: Task, viewId: ViewId): boolean {
  switch (viewId) {
    case "all":
      return true;
    case "running":
      return task.status === "downloading";
    case "paused":
      return task.status === "paused";
    case "completed":
      return task.status === "completed";
    case "unfinished":
      return task.status !== "completed";
    case "waiting":
      return task.status === "queued" || task.status === "scheduled";
    case "finished":
      return task.status === "completed";
    default:
      return task.category === viewId;
  }
}

export function selectTasks(
  tasks: readonly Task[],
  viewId: ViewId,
): readonly Task[] {
  return tasks.filter((task) => taskInView(task, viewId));
}

export function selectVisibleTasks(
  tasks: readonly Task[],
  viewId: ViewId,
  query: string,
): readonly Task[] {
  const needle = query.trim().toLowerCase();
  const scoped = selectTasks(tasks, viewId);
  if (needle === "") return scoped;
  return scoped.filter(
    (task) =>
      task.fileName.toLowerCase().includes(needle) ||
      task.url.toLowerCase().includes(needle),
  );
}

export function countForView(tasks: readonly Task[], viewId: ViewId): number {
  return tasks.filter((task) => taskInView(task, viewId)).length;
}
