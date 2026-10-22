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
  | "queued"
  | "scheduled"
  | "downloading"
  | "paused"
  | "completed"
  | "error";

/** The host's TaskView, with no invented client-only fields. */
export interface Task {
  readonly id: number;
  readonly url: string;
  readonly finalUrl: string | null;
  readonly fileName: string;
  readonly destPath: string;
  readonly category: TagId;
  readonly status: TaskStatus;
  readonly priority: number;
  readonly totalBytes: number | null;
  readonly downloaded: number;
  readonly speedBps: number;
  readonly etaSeconds: number | null;
  readonly errorMessage: string | null;
  readonly attempts: number;
  readonly scheduleId: number | null;
  readonly sha256: string | null;
  readonly createdAt: number;
  readonly updatedAt: number;
}

export interface ProgressUpdate {
  readonly id: number;
  readonly downloaded: number;
  readonly speedBps: number;
  readonly etaSeconds: number | null;
  readonly totalBytes: number | null;
}

/** Payloads are tagged and routed on four separate rapids:// channels. */
export type HostEvent =
  | { readonly kind: "taskChanged"; readonly task: Task }
  | { readonly kind: "taskRemoved"; readonly id: number }
  | { readonly kind: "progress"; readonly updates: readonly ProgressUpdate[] }
  | { readonly kind: "queueChanged" };

export type StoreStatus = "loading" | "ready" | "failed";

export type SniffSource = "video" | "videoSource" | "link" | "navigation";

/** An in-memory suggestion from the inspector, not a download task. */
export interface SniffCandidate {
  readonly session: number;
  readonly pageUrl: string;
  readonly url: string;
  readonly source: SniffSource;
}

export interface AppInfo {
  readonly name: string;
  readonly version: string;
}

/** The host's settings keys remain snake_case when written individually. */
export type SettingName =
  | "max_parallel"
  | "max_connections_total"
  | "global_speed_limit"
  | "resume_on_start"
  | "retry_max_attempts"
  | "retry_base_ms"
  | "retry_cap_ms"
  | "default_dest_dir";

export interface Settings {
  readonly maxParallel: number;
  readonly maxConnectionsTotal: number;
  readonly globalSpeedLimit: number | null;
  readonly resumeOnStart: boolean;
  readonly retryMaxAttempts: number;
  readonly retryBaseMs: number;
  readonly retryCapMs: number;
  readonly defaultDestDir: string;
}

export interface ScheduleView {
  readonly id: number;
  readonly label: string;
  readonly kind: "once" | "daily" | "weekly";
  readonly startAt: number;
  readonly windowMinutes: number;
  readonly weekdays: number;
  readonly enabled: boolean;
}

export interface ScheduleCreateInput {
  readonly label: string;
  readonly kind: ScheduleView["kind"];
  readonly startAt: number;
  readonly windowMinutes: number;
  readonly weekdays: number;
  readonly enabled: boolean;
}

export interface TaskAddInput {
  readonly url: string;
  readonly category: TagId;
  readonly destDir?: string;
  readonly priority?: number;
  readonly connections?: number;
  readonly speedLimitBps?: number;
  readonly expectedSha256?: string;
  readonly scheduleId?: number;
}

export interface QueueCounts {
  readonly queued: number;
  readonly scheduled: number;
  readonly downloading: number;
  readonly paused: number;
  readonly completed: number;
  readonly failed: number;
}
