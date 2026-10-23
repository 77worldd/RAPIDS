import type { SortKey } from "../lib/preferences";
import { progressFraction } from "../lib/format";
import type { Task, ViewId } from "../types";

function taskInView(task: Task, viewId: ViewId): boolean {
  switch (viewId) {
    case "all":
      return true;
    case "downloading":
      return task.status === "downloading";
    case "queued":
      return task.status === "queued" || task.status === "scheduled";
    case "completed":
      return task.status === "completed";
    case "paused":
      return task.status === "paused";
    case "failed":
      return task.status === "error";
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
      task.url.toLowerCase().includes(needle) ||
      task.destPath.toLowerCase().includes(needle) ||
      task.category.toLowerCase().includes(needle),
  );
}

export function countForView(tasks: readonly Task[], viewId: ViewId): number {
  return tasks.filter((task) => taskInView(task, viewId)).length;
}

const STATUS_ORDER: Readonly<Record<Task["status"], number>> = {
  downloading: 0,
  queued: 1,
  scheduled: 2,
  paused: 3,
  error: 4,
  completed: 5,
};

export function sortTasks(tasks: readonly Task[], sort: SortKey): readonly Task[] {
  const ordered = [...tasks];
  switch (sort) {
    case "newest":
      ordered.sort((left, right) => right.createdAt - left.createdAt || right.id - left.id);
      break;
    case "oldest":
      ordered.sort((left, right) => left.createdAt - right.createdAt || left.id - right.id);
      break;
    case "name":
      ordered.sort(
        (left, right) =>
          left.fileName.localeCompare(right.fileName, undefined, { sensitivity: "base" }) ||
          left.id - right.id,
      );
      break;
    case "size":
      ordered.sort(
        (left, right) =>
          (right.totalBytes ?? right.downloaded) - (left.totalBytes ?? left.downloaded) ||
          right.id - left.id,
      );
      break;
    case "progress":
      ordered.sort((left, right) => {
        const leftProgress = progressFraction(left) ?? -1;
        const rightProgress = progressFraction(right) ?? -1;
        return rightProgress - leftProgress || right.id - left.id;
      });
      break;
    case "status":
      ordered.sort(
        (left, right) =>
          STATUS_ORDER[left.status] - STATUS_ORDER[right.status] || right.id - left.id,
      );
      break;
  }
  return ordered;
}
