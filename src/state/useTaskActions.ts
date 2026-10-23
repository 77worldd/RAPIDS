import { useMemo } from "react";
import {
  cancelTask,
  deleteTask,
  describeError,
  pauseTask,
  reorderTask,
  resumeTask,
  retryTask,
} from "../lib/ipc";
import {
  openDownloadedFile,
  revealDownloadedFile,
  writeClipboardText,
} from "../lib/native";
import type { Task } from "../types";
import { useCommand, useStoreDispatch } from "./StoreProvider";

export interface TaskActions {
  readonly pause: (task: Task) => void;
  readonly resume: (task: Task) => void;
  readonly retry: (task: Task) => void;
  readonly cancel: (task: Task) => void;
  readonly remove: (task: Task, removeFile: boolean) => void;
  readonly reorder: (task: Task, delta: number) => void;
  readonly openFile: (task: Task) => void;
  readonly revealFile: (task: Task) => void;
  readonly copyUrl: (task: Task) => void;
}

export function useTaskActions(): TaskActions {
  const command = useCommand();
  const dispatch = useStoreDispatch();

  return useMemo<TaskActions>(
    () => ({
      pause: (task) => command(() => pauseTask(task.id)),
      resume: (task) => command(() => resumeTask(task.id)),
      retry: (task) => command(() => retryTask(task.id)),
      cancel: (task) => {
        if (
          !window.confirm(
            `Cancel “${task.fileName}” and remove it from the list? Partial data stays on disk.`,
          )
        ) {
          return;
        }
        command(() => cancelTask(task.id));
      },
      remove: (task, removeFile) => {
        const message = removeFile
          ? `Delete “${task.fileName}” from disk and remove it from the list?`
          : `Remove “${task.fileName}” from the list? The file stays on disk.`;
        if (!window.confirm(message)) return;
        command(() => deleteTask(task.id, removeFile));
      },
      reorder: (task, delta) => {
        command(() => reorderTask(task.id, task.priority + delta));
      },
      openFile: (task) => {
        void openDownloadedFile(task.destPath).catch((error: unknown) => {
          dispatch({ type: "notice/set", message: describeError(error) });
        });
      },
      revealFile: (task) => {
        void revealDownloadedFile(task.destPath).catch((error: unknown) => {
          dispatch({ type: "notice/set", message: describeError(error) });
        });
      },
      copyUrl: (task) => {
        void writeClipboardText(task.finalUrl ?? task.url).catch((error: unknown) => {
          dispatch({ type: "notice/set", message: describeError(error) });
        });
      },
    }),
    [command, dispatch],
  );
}
