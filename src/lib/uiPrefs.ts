const STORAGE_KEY = "rapids.ui";
const SCHEMA_VERSION = 1;

interface StoredUiPrefs {
  readonly v: number;
  readonly sidebarCollapsed: boolean;
}

function isStoredUiPrefs(value: unknown): value is StoredUiPrefs {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const candidate = value as { v?: unknown; sidebarCollapsed?: unknown };
  return (
    candidate.v === SCHEMA_VERSION &&
    typeof candidate.sidebarCollapsed === "boolean"
  );
}

export function readSidebarCollapsed(): boolean {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw === null) {
      return false;
    }
    const parsed: unknown = JSON.parse(raw);
    return isStoredUiPrefs(parsed) ? parsed.sidebarCollapsed : false;
  } catch {
    return false;
  }
}

export function writeSidebarCollapsed(sidebarCollapsed: boolean): void {
  try {
    const prefs: StoredUiPrefs = { v: SCHEMA_VERSION, sidebarCollapsed };
    localStorage.setItem(STORAGE_KEY, JSON.stringify(prefs));
  } catch {
    // Storage may be unavailable; the preference stays session-only.
  }
}
