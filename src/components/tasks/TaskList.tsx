import { useMemo } from "react";
import {
  cancelTask,
  deleteTask,
  pauseTask,
  reorderTask,
  resumeTask,
  retryTask,
} from "../../lib/ipc";
import { useCommand } from "../../state/StoreProvider";
import type { Task } from "../../types";
import { TaskRow, type TaskActions } from "./TaskRow";

export function TaskList({ tasks }: { readonly tasks: readonly Task[] }) {
  const command = useCommand();
  const actions = useMemo<TaskActions>(
    () => ({
      pause: (id) => command(() => pauseTask(id)),
      resume: (id) => command(() => resumeTask(id)),
      retry: (id) => command(() => retryTask(id)),
      cancel: (id) => command(() => cancelTask(id)),
      remove: (id, removeFile) =>
        command(() => deleteTask(id, removeFile)),
      reorder: (id, priority) =>
        command(() => reorderTask(id, priority)),
    }),
    [command],
  );

  return (
    <ul aria-label="Downloads" className="divide-y divide-zinc-800/70">
      {tasks.map((task) => (
        <TaskRow key={task.id} task={task} actions={actions} />
      ))}
    </ul>
  );
}
