import type { Task } from "../types";

const BYTE_UNITS = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"] as const;

export function formatBytes(bytes: number): string {
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < BYTE_UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  if (unit === 0) return `${bytes} B`;
  return `${value.toFixed(value >= 10 ? 1 : 2)} ${BYTE_UNITS[unit] ?? "B"}`;
}

export function formatSpeed(bytesPerSecond: number): string {
  return `${formatBytes(bytesPerSecond)}/s`;
}

export function formatEta(seconds: number | null): string {
  if (seconds === null) return "—";
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ${seconds % 60}s`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ${minutes % 60}m`;
  return `${Math.floor(hours / 24)}d ${hours % 24}h`;
}

/** Unknown total means no percentage. A verified empty file is complete. */
export function progressFraction(
  task: Pick<Task, "downloaded" | "totalBytes" | "status">,
): number | null {
  if (task.totalBytes === null) return null;
  if (task.totalBytes === 0) return task.status === "completed" ? 1 : null;
  return Math.min(1, Math.max(0, task.downloaded / task.totalBytes));
}

export function shortHash(hash: string): string {
  return hash.length > 20 ? `${hash.slice(0, 8)}…${hash.slice(-8)}` : hash;
}
