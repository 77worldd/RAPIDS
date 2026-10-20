import type { Task, ViewId } from "../types";

export function selectTasks(
  tasks: readonly Task[],
  viewId: ViewId,
): readonly Task[] {
  switch (viewId) {
    case "all":
      return tasks;
    case "running":
      return tasks.filter((task) => task.status === "downloading");
    case "paused":
      return tasks.filter((task) => task.status === "paused");
    case "completed":
      return tasks.filter((task) => task.status === "completed");
    case "unfinished":
      return tasks.filter((task) => task.status !== "completed");
    case "waiting":
      return tasks.filter((task) => task.status === "queued");
    case "finished":
      return tasks.filter((task) => task.status === "completed");
    default:
      return tasks.filter((task) => task.category === viewId);
  }
}

export function selectVisibleTasks(
  tasks: readonly Task[],
  viewId: ViewId,
  query: string,
): readonly Task[] {
  const scoped = selectTasks(tasks, viewId);
  const needle = query.trim().toLowerCase();
  if (needle === "") {
    return scoped;
  }
  return scoped.filter(
    (task) =>
      task.fileName.toLowerCase().includes(needle) ||
      task.tags.some((tag) => tag.toLowerCase().includes(needle)),
  );
}

export function countForView(tasks: readonly Task[], viewId: ViewId): number {
  return selectTasks(tasks, viewId).length;
}
