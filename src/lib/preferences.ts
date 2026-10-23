import type { TagId } from "../types";
import { isAbsoluteFolder } from "./addDraft";

const CATEGORY_DIRS_KEY = "rapids.categoryDirs.v1";
const WORKSPACE_KEY = "rapids.workspace.v1";

export type CategoryDirs = Partial<Record<TagId, string>>;

export type SortKey = "newest" | "oldest" | "name" | "size" | "progress" | "status";

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
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
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
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
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

export function destinationFor(
  category: TagId,
  categoryDirs: CategoryDirs,
): string | null {
  const value = categoryDirs[category];
  return value === undefined || value.trim() === "" ? null : value;
}
