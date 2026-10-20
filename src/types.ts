export type CoreViewId =
  | "all"
  | "running"
  | "paused"
  | "completed"
  | "unfinished"
  | "waiting"
  | "finished";

export type TagId = "apps" | "movies" | "music" | "images" | "other";

export type ViewId = CoreViewId | TagId;

export type TaskStatus =
  "queued" | "downloading" | "paused" | "completed" | "error";

export interface Task {
  readonly id: string;
  readonly fileName: string;
  readonly status: TaskStatus;
  readonly totalBytes: number | null;
  readonly speedBps: number;
  readonly etaSeconds: number | null;
  readonly category: TagId;
  readonly tags: readonly string[];
  readonly favorite: boolean;
  readonly addedAt: number;
}

export type StoreStatus = "loading" | "ready";

export interface AppInfo {
  readonly name: string;
  readonly version: string;
}
