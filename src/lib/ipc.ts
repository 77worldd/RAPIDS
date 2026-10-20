import { invoke } from "@tauri-apps/api/core";
import type { AppInfo } from "../types";

export function fetchAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("app_version");
}
