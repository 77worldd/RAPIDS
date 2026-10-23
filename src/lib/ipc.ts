import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { parsePendingDeepLink, type PendingDeepLink } from "./deeplink";
import { toInspectorUrl, toSniffCandidate } from "./sniff";
import type {
  AppInfo,
  HostEvent,
  ProgressUpdate,
  QueueCounts,
  ScheduleCreateInput,
  ScheduleView,
  SettingName,
  Settings,
  SniffCandidate,
  TagId,
  Task,
  TaskAddInput,
  TaskStatus,
} from "../types";

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isInt(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value);
}

function isNonNegative(value: unknown): value is number {
  return isInt(value) && value >= 0;
}

function isId(value: unknown): value is number {
  return isInt(value) && value > 0;
}

function isNullableString(value: unknown): value is string | null {
  return value === null || typeof value === "string";
}

function isNullableNonNegative(value: unknown): value is number | null {
  return value === null || isNonNegative(value);
}

function isCategory(value: unknown): value is TagId {
  return (
    value === "apps" ||
    value === "movies" ||
    value === "music" ||
    value === "images" ||
    value === "documents" ||
    value === "archives" ||
    value === "other"
  );
}

function isStatus(value: unknown): value is TaskStatus {
  return (
    value === "queued" ||
    value === "scheduled" ||
    value === "downloading" ||
    value === "paused" ||
    value === "completed" ||
    value === "error"
  );
}

function isTask(value: unknown): value is Task {
  if (!isRecord(value)) return false;
  return (
    isId(value.id) &&
    typeof value.url === "string" &&
    isNullableString(value.finalUrl) &&
    typeof value.fileName === "string" &&
    typeof value.destPath === "string" &&
    isCategory(value.category) &&
    isStatus(value.status) &&
    isInt(value.priority) &&
    isNullableNonNegative(value.totalBytes) &&
    isNonNegative(value.downloaded) &&
    isNonNegative(value.speedBps) &&
    isNullableNonNegative(value.etaSeconds) &&
    isNullableString(value.errorMessage) &&
    isNonNegative(value.attempts) &&
    (value.scheduleId === null || isId(value.scheduleId)) &&
    isNullableString(value.sha256) &&
    isNonNegative(value.createdAt) &&
    isNonNegative(value.updatedAt)
  );
}

function isProgress(value: unknown): value is ProgressUpdate {
  if (!isRecord(value)) return false;
  return (
    isId(value.id) &&
    isNonNegative(value.downloaded) &&
    isNonNegative(value.speedBps) &&
    isNullableNonNegative(value.etaSeconds) &&
    isNullableNonNegative(value.totalBytes)
  );
}

function isSettings(value: unknown): value is Settings {
  if (!isRecord(value)) return false;
  return (
    isId(value.maxParallel) &&
    isId(value.maxConnectionsTotal) &&
    isNullableNonNegative(value.globalSpeedLimit) &&
    typeof value.resumeOnStart === "boolean" &&
    isNonNegative(value.retryMaxAttempts) &&
    isNonNegative(value.retryBaseMs) &&
    isNonNegative(value.retryCapMs) &&
    typeof value.defaultDestDir === "string"
  );
}

function isSchedule(value: unknown): value is ScheduleView {
  if (!isRecord(value)) return false;
  return (
    isId(value.id) &&
    typeof value.label === "string" &&
    (value.kind === "once" ||
      value.kind === "daily" ||
      value.kind === "weekly") &&
    isInt(value.startAt) &&
    isNonNegative(value.windowMinutes) &&
    isNonNegative(value.weekdays) &&
    typeof value.enabled === "boolean"
  );
}

function isQueueCounts(value: unknown): value is QueueCounts {
  if (!isRecord(value)) return false;
  return (
    isNonNegative(value.queued) &&
    isNonNegative(value.scheduled) &&
    isNonNegative(value.downloading) &&
    isNonNegative(value.paused) &&
    isNonNegative(value.completed) &&
    isNonNegative(value.failed)
  );
}

function listOf<T>(
  value: unknown,
  isItem: (candidate: unknown) => candidate is T,
  label: string,
): readonly T[] {
  if (!Array.isArray(value) || !value.every(isItem)) {
    throw new Error(`The download service returned an invalid ${label}.`);
  }
  return value;
}

async function checked<T>(
  command: string,
  isValue: (candidate: unknown) => candidate is T,
  args?: Record<string, unknown>,
): Promise<T> {
  const result: unknown = await invoke(command, args);
  if (!isValue(result)) {
    throw new Error(
      `The download service returned an invalid ${command} reply.`,
    );
  }
  return result;
}

function send(command: string, args?: Record<string, unknown>): Promise<void> {
  return invoke<void>(command, args);
}

export function fetchAppInfo(): Promise<AppInfo> {
  return checked(
    "app_version",
    (value): value is AppInfo =>
      isRecord(value) &&
      typeof value.name === "string" &&
      typeof value.version === "string",
  );
}

export function fetchDownloadDefaultDir(): Promise<string> {
  return checked(
    "download_default_dir",
    (value): value is string => typeof value === "string" && value !== "",
  );
}

export async function listTasks(): Promise<readonly Task[]> {
  const result: unknown = await invoke("tasks_list");
  return listOf(result, isTask, "task list");
}

export function addTask(request: TaskAddInput): Promise<Task> {
  return checked("task_add", isTask, { request });
}

export async function openSniffInspector(input: string): Promise<void> {
  const url = toInspectorUrl(input);
  await send("sniff_open", { url });
}

export function pauseTask(id: number): Promise<void> {
  return send("task_pause", { id });
}

export function resumeTask(id: number): Promise<void> {
  return send("task_resume", { id });
}

export function retryTask(id: number): Promise<void> {
  return send("task_retry", { id });
}

export function cancelTask(id: number): Promise<void> {
  return send("task_cancel", { id });
}

export function deleteTask(id: number, removeFile: boolean): Promise<void> {
  return send("task_delete", { id, removeFile });
}

export function reorderTask(id: number, priority: number): Promise<void> {
  if (!Number.isSafeInteger(priority)) {
    return Promise.reject(new Error("Priority cannot be changed further."));
  }
  return send("task_reorder", { id, priority });
}

export function pauseQueue(): Promise<void> {
  return send("queue_pause");
}

export function resumeQueue(): Promise<void> {
  return send("queue_resume");
}

export function queuePaused(): Promise<boolean> {
  return checked(
    "queue_paused",
    (value): value is boolean => typeof value === "boolean",
  );
}

export function getSettings(): Promise<Settings> {
  return checked("settings_get", isSettings);
}

export function updateSetting(
  name: SettingName,
  value: unknown,
): Promise<Settings> {
  return checked("settings_update", isSettings, { name, value });
}

export async function listSchedules(): Promise<readonly ScheduleView[]> {
  const result: unknown = await invoke("schedules_list");
  return listOf(result, isSchedule, "schedule list");
}

export function createSchedule(
  input: ScheduleCreateInput,
): Promise<ScheduleView> {
  return checked("schedule_create", isSchedule, { input });
}

export function setScheduleEnabled(
  id: number,
  enabled: boolean,
): Promise<void> {
  return send("schedule_set_enabled", { id, enabled });
}

export function deleteSchedule(id: number): Promise<void> {
  return send("schedule_delete", { id });
}

export function queueCounts(): Promise<QueueCounts> {
  return checked("queue_counts", isQueueCounts);
}

function toHostEvent(value: unknown): HostEvent | null {
  if (!isRecord(value)) return null;
  switch (value.kind) {
    case "taskChanged":
      return isTask(value.task)
        ? { kind: "taskChanged", task: value.task }
        : null;
    case "taskRemoved":
      return isId(value.id) ? { kind: "taskRemoved", id: value.id } : null;
    case "progress":
      return Array.isArray(value.updates) && value.updates.every(isProgress)
        ? { kind: "progress", updates: value.updates }
        : null;
    case "queueChanged":
      return { kind: "queueChanged" };
    default:
      return null;
  }
}

const CHANNELS = [
  ["rapids://progress", "progress"],
  ["rapids://task-changed", "taskChanged"],
  ["rapids://task-removed", "taskRemoved"],
  ["rapids://queue-changed", "queueChanged"],
] as const;

/** Subscribe to the full host stream; undo partial registration on failure. */
export async function subscribeHostEvents(
  onEvent: (event: HostEvent) => void,
  onError: (error: Error) => void,
): Promise<UnlistenFn> {
  const unlisten: UnlistenFn[] = [];
  try {
    for (const [channel, kind] of CHANNELS) {
      const off = await listen<unknown>(channel, (message) => {
        const event = toHostEvent(message.payload);
        if (event !== null && event.kind === kind) {
          onEvent(event);
        } else {
          onError(
            new Error(`The download service sent an invalid ${channel} event.`),
          );
        }
      });
      unlisten.push(off);
    }
    return () => {
      for (const off of unlisten) off();
    };
  } catch (error) {
    for (const off of unlisten) off();
    throw error;
  }
}

export async function subscribeSniffCandidates(
  onCandidate: (candidate: SniffCandidate) => void,
  onError: (error: Error) => void,
): Promise<UnlistenFn> {
  return listen<unknown>("rapids://sniff-candidate", (message) => {
    const candidate = toSniffCandidate(message.payload);
    if (candidate === null) {
      onError(new Error("The inspector sent an invalid media candidate."));
    } else {
      onCandidate(candidate);
    }
  });
}

/**
 * The host queues both warm and cold links. Draining is atomic, so this is the
 * single authoritative path; browser events only wake the bridge to drain again.
 */
export async function takePendingDeepLinks(): Promise<
  readonly PendingDeepLink[]
> {
  const result: unknown = await invoke("deeplink_take_pending");
  if (!Array.isArray(result)) {
    throw new Error(
      "The download service returned an invalid browser handoff queue.",
    );
  }
  const pending: PendingDeepLink[] = [];
  for (const entry of result) {
    const parsed = parsePendingDeepLink(entry);
    if (parsed !== null) pending.push(parsed);
  }
  return pending;
}

const DEEP_LINK_CHANNELS = ["deeplink://add", "deeplink://error"] as const;

/** Payloads are ignored by design; the bridge re-reads the validated queue. */
export async function subscribeDeepLinkSignals(
  onSignal: () => void,
): Promise<UnlistenFn> {
  const unlisten: UnlistenFn[] = [];
  try {
    for (const channel of DEEP_LINK_CHANNELS) {
      const off = await listen(channel, () => onSignal());
      unlisten.push(off);
    }
    return () => {
      for (const off of unlisten) off();
    };
  } catch (error) {
    for (const off of unlisten) off();
    throw error;
  }
}

export function describeError(error: unknown): string {
  if (typeof error === "string" && error.trim() !== "") return error;
  if (error instanceof Error && error.message.trim() !== "")
    return error.message;
  return "The download service did not respond.";
}
