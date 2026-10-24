import type { TagId } from "../types";
import { isAbsoluteFolder } from "./addDraft";

const CATEGORY_DIRS_KEY = "rapids.categoryDirs.v1";
const WORKSPACE_KEY = "rapids.workspace.v1";
const DEEP_LINK_CONFIRM_KEY = "rapids.deeplinkConfirm.v1";
const THEME_KEY = "rapids.theme.v1";

export type CategoryDirs = Partial<Record<TagId, string>>;

export type SortKey =
  "newest" | "oldest" | "name" | "size" | "progress" | "status";

export interface WorkspacePrefs {
  readonly sort: SortKey;
}

function isTagId(value: string): value is TagId {
  return (
    value === "movies" ||
    value === "music" ||
    value === "images" ||
    value === "documents" ||
    value === "archives" ||
    value === "apps" ||
    value === "other"
  );
}

export function readCategoryDirs(): CategoryDirs {
  try {
    const raw = localStorage.getItem(CATEGORY_DIRS_KEY);
    if (raw === null) return {};
    const parsed: unknown = JSON.parse(raw);
    if (
      typeof parsed !== "object" ||
      parsed === null ||
      Array.isArray(parsed)
    ) {
      return {};
    }
    const result: CategoryDirs = {};
    for (const [key, value] of Object.entries(parsed)) {
      if (
        isTagId(key) &&
        typeof value === "string" &&
        value.trim() !== "" &&
        isAbsoluteFolder(value.trim())
      ) {
        result[key] = value.trim();
      }
    }
    return result;
  } catch {
    return {};
  }
}

export function writeCategoryDirs(dirs: CategoryDirs): void {
  try {
    const cleaned: CategoryDirs = {};
    for (const [key, value] of Object.entries(dirs)) {
      if (isTagId(key) && typeof value === "string" && value.trim() !== "") {
        cleaned[key] = value.trim();
      }
    }
    localStorage.setItem(CATEGORY_DIRS_KEY, JSON.stringify(cleaned));
  } catch {
    // Storage can be unavailable; the current session still uses its values.
  }
}

export function readWorkspacePrefs(): WorkspacePrefs {
  const fallback: WorkspacePrefs = { sort: "newest" };
  try {
    const raw = localStorage.getItem(WORKSPACE_KEY);
    if (raw === null) return fallback;
    const parsed: unknown = JSON.parse(raw);
    if (
      typeof parsed !== "object" ||
      parsed === null ||
      Array.isArray(parsed)
    ) {
      return fallback;
    }
    const sort = (parsed as { sort?: unknown }).sort;
    if (
      sort === "newest" ||
      sort === "oldest" ||
      sort === "name" ||
      sort === "size" ||
      sort === "progress" ||
      sort === "status"
    ) {
      return { sort };
    }
    return fallback;
  } catch {
    return fallback;
  }
}

export function writeWorkspacePrefs(prefs: WorkspacePrefs): void {
  try {
    localStorage.setItem(WORKSPACE_KEY, JSON.stringify(prefs));
  } catch {
    // Storage can be unavailable; sorting still works for this session.
  }
}

/** Browser links require confirmation unless the user explicitly opts out. */
export function readDeepLinkConfirm(): boolean {
  try {
    return localStorage.getItem(DEEP_LINK_CONFIRM_KEY) !== "false";
  } catch {
    return true;
  }
}

export function writeDeepLinkConfirm(confirm: boolean): void {
  try {
    localStorage.setItem(DEEP_LINK_CONFIRM_KEY, confirm ? "true" : "false");
  } catch {
    // Storage can be unavailable; the in-memory setting still applies.
  }
}

export type ThemeChoice = "system" | "light" | "dark";

export function readTheme(): ThemeChoice {
  try {
    const raw = localStorage.getItem(THEME_KEY);
    return raw === "light" || raw === "dark" ? raw : "system";
  } catch {
    return "system";
  }
}

/** Applies the preference to the document; "system" follows the OS. */
export function applyTheme(theme: ThemeChoice): void {
  const root = document.documentElement;
  if (theme === "system") {
    delete root.dataset.theme;
  } else {
    root.dataset.theme = theme;
  }
}

export function writeTheme(theme: ThemeChoice): void {
  try {
    localStorage.setItem(THEME_KEY, theme);
  } catch {
    // Storage can be unavailable; the current session still uses the theme.
  }
}

export function destinationFor(
  category: TagId,
  categoryDirs: CategoryDirs,
): string | null {
  const value = categoryDirs[category];
  return value === undefined || value.trim() === "" ? null : value;
}
