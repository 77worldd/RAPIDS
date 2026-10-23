import {
  CalendarClock,
  Check,
  FolderCog,
  Gauge,
  Loader2,
  Plus,
  Settings2,
  Trash2,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { CATEGORIES } from "../../config/categories";
import { isAbsoluteFolder } from "../../lib/addDraft";
import { cn } from "../../lib/cn";
import {
  createSchedule,
  deleteSchedule,
  describeError,
  getSettings,
  listSchedules,
  setScheduleEnabled,
  updateSetting,
} from "../../lib/ipc";
import { pickFolder } from "../../lib/native";
import {
  applyTheme,
  readCategoryDirs,
  readDeepLinkConfirm,
  readTheme,
  writeCategoryDirs,
  writeDeepLinkConfirm,
  writeTheme,
  type CategoryDirs,
  type ThemeChoice,
} from "../../lib/preferences";
import type {
  ScheduleCreateInput,
  ScheduleView,
  SettingName,
  Settings,
} from "../../types";
import { Button } from "../ui/Button";
import { DialogWindow } from "../ui/DialogWindow";
import { IconButton } from "../ui/IconButton";
import { Toggle } from "../ui/Toggle";

const FIELD =
  "h-9 w-full rounded-lg border border-line bg-canvas/55 px-3 text-[12.5px] text-ink outline-none transition-[border-color,background-color,box-shadow] duration-150 ease-out focus:border-ink/60 focus:bg-canvas focus:ring-2 focus:ring-ink/15";
const LABEL = "mb-1.5 block text-[12px] font-medium text-ink-soft";
const HELP = "mt-1.5 text-[11px] leading-relaxed text-muted";

type TabId = "general" | "folders" | "transfers" | "schedules";

interface SettingsDraft {
  readonly maxParallel: string;
  readonly maxConnections: string;
  readonly globalSpeedKbps: string;
  readonly resumeOnStart: boolean;
  readonly retryMaxAttempts: string;
  readonly defaultDestDir: string;
}

const TABS: readonly { id: TabId; label: string; icon: typeof Settings2 }[] = [
  { id: "general", label: "General", icon: Settings2 },
  { id: "folders", label: "Folders", icon: FolderCog },
  { id: "transfers", label: "Transfers", icon: Gauge },
  { id: "schedules", label: "Scheduling", icon: CalendarClock },
];

const WEEKDAYS: readonly { bit: number; label: string }[] = [
  { bit: 1, label: "Mon" },
  { bit: 2, label: "Tue" },
  { bit: 4, label: "Wed" },
  { bit: 8, label: "Thu" },
  { bit: 16, label: "Fri" },
  { bit: 32, label: "Sat" },
  { bit: 64, label: "Sun" },
];

function draftFrom(settings: Settings): SettingsDraft {
  return {
    maxParallel: String(settings.maxParallel),
    maxConnections: String(settings.maxConnectionsTotal),
    globalSpeedKbps:
      settings.globalSpeedLimit === null
        ? ""
        : String(Math.round(settings.globalSpeedLimit / 1024)),
    resumeOnStart: settings.resumeOnStart,
    retryMaxAttempts: String(settings.retryMaxAttempts),
    defaultDestDir: settings.defaultDestDir,
  };
}

function integerInRange(
  value: string,
  label: string,
  lowest: number,
  highest: number,
): number {
  const text = value.trim();
  if (!/^\d+$/.test(text)) {
    throw new Error(`${label} must be a whole number.`);
  }
  const number = Number(text);
  if (number < lowest || number > highest) {
    throw new Error(`${label} must be between ${lowest} and ${highest}.`);
  }
  return number;
}

function localInputValue(date: Date): string {
  const pad = (part: number) => String(part).padStart(2, "0");
  return [
    date.getFullYear(),
    "-",
    pad(date.getMonth() + 1),
    "-",
    pad(date.getDate()),
    "T",
    pad(date.getHours()),
    ":",
    pad(date.getMinutes()),
  ].join("");
}

function scheduleKindLabel(kind: ScheduleView["kind"]): string {
  if (kind === "once") return "Once";
  if (kind === "daily") return "Daily";
  return "Weekly";
}

function weekdayNames(mask: number): string {
  const selected = WEEKDAYS.filter((day) => mask & day.bit).map(
    (day) => day.label,
  );
  if (selected.length === 0) return "No days selected";
  if (selected.length === 7) return "Every day";
  return selected.join(", ");
}

function scheduleWhen(schedule: ScheduleView): string {
  return new Date(schedule.startAt).toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  });
}

function scheduleWindow(minutes: number): string {
  if (minutes === 0) return "No closing time";
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest === 0 ? `${hours} hr` : `${hours} hr ${rest} min`;
}

function SettingsSkeleton() {
  return (
    <div className="space-y-4" aria-label="Loading settings">
      {[0, 1, 2].map((row) => (
        <div key={row} className="space-y-2">
          <div className="h-3 w-24 animate-pulse rounded bg-elevated" />
          <div className="h-9 w-full animate-pulse rounded-lg bg-elevated/70" />
        </div>
      ))}
    </div>
  );
}

export function SettingsDialog({ onClose }: { readonly onClose: () => void }) {
  const [tab, setTab] = useState<TabId>("general");
  const [settings, setSettings] = useState<Settings | null>(null);
  const [draft, setDraft] = useState<SettingsDraft | null>(null);
  const [categoryDirs, setCategoryDirs] =
    useState<CategoryDirs>(readCategoryDirs);
  const [confirmBrowserLinks, setConfirmBrowserLinks] =
    useState(readDeepLinkConfirm);
  const [theme, setTheme] = useState<ThemeChoice>(readTheme);
  const [schedules, setSchedules] = useState<readonly ScheduleView[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadAttempt, setLoadAttempt] = useState(0);
  const [saving, setSaving] = useState(false);
  const [browsing, setBrowsing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [scheduleError, setScheduleError] = useState<string | null>(null);
  const [scheduleBusy, setScheduleBusy] = useState<number | null>(null);
  const [newLabel, setNewLabel] = useState("");
  const [newKind, setNewKind] = useState<ScheduleView["kind"]>("once");
  const [newWeekdays, setNewWeekdays] = useState(31);
  const [newStart, setNewStart] = useState(() =>
    localInputValue(new Date(Date.now() + 60 * 60 * 1000)),
  );
  const [newWindow, setNewWindow] = useState("60");
  const [newEnabled, setNewEnabled] = useState(true);
  const [creating, setCreating] = useState(false);

  useEffect(() => {
    let active = true;
    void Promise.allSettled([getSettings(), listSchedules()]).then(
      ([settingsResult, scheduleResult]) => {
        if (!active) return;
        setError(null);
        setScheduleError(null);
        if (settingsResult.status === "fulfilled") {
          setSettings(settingsResult.value);
          setDraft(draftFrom(settingsResult.value));
        } else {
          setError(describeError(settingsResult.reason));
        }
        if (scheduleResult.status === "fulfilled") {
          setSchedules(scheduleResult.value);
        } else {
          setScheduleError(describeError(scheduleResult.reason));
        }
        setLoading(false);
      },
    );
    return () => {
      active = false;
    };
  }, [loadAttempt]);

  useEffect(() => {
    if (saved === null) return undefined;
    const timeout = window.setTimeout(() => setSaved(null), 2600);
    return () => window.clearTimeout(timeout);
  }, [saved]);

  const dirty = useMemo(() => {
    if (settings === null || draft === null) return false;
    return (
      draft.maxParallel !== String(settings.maxParallel) ||
      draft.maxConnections !== String(settings.maxConnectionsTotal) ||
      draft.globalSpeedKbps !==
        (settings.globalSpeedLimit === null
          ? ""
          : String(Math.round(settings.globalSpeedLimit / 1024))) ||
      draft.resumeOnStart !== settings.resumeOnStart ||
      draft.retryMaxAttempts !== String(settings.retryMaxAttempts) ||
      draft.defaultDestDir !== settings.defaultDestDir ||
      JSON.stringify(categoryDirs) !== JSON.stringify(readCategoryDirs()) ||
      confirmBrowserLinks !== readDeepLinkConfirm()
    );
  }, [categoryDirs, confirmBrowserLinks, draft, settings]);

  function updateDraft<K extends keyof SettingsDraft>(
    key: K,
    value: SettingsDraft[K],
  ): void {
    setDraft((current) =>
      current === null ? current : { ...current, [key]: value },
    );
    setError(null);
    setSaved(null);
  }

  function updateCategoryDir(
    categoryId: keyof CategoryDirs,
    value: string,
  ): void {
    setCategoryDirs((current) => {
      const next: CategoryDirs = { ...current };
      if (value.trim() === "") {
        delete next[categoryId];
      } else {
        next[categoryId] = value;
      }
      return next;
    });
    setSaved(null);
  }

  async function browseDefault(): Promise<void> {
    if (browsing) return;
    setBrowsing(true);
    setError(null);
    try {
      const folder = await pickFolder(
        draft?.defaultDestDir ?? settings?.defaultDestDir ?? "",
        "Choose the default download folder",
      );
      if (folder !== null) updateDraft("defaultDestDir", folder);
    } catch (cause) {
      setError(describeError(cause));
    } finally {
      setBrowsing(false);
    }
  }

  async function browseCategory(
    categoryId: keyof CategoryDirs,
    current: string,
  ): Promise<void> {
    if (browsing) return;
    setBrowsing(true);
    setError(null);
    try {
      const folder = await pickFolder(current, "Choose a category folder");
      if (folder !== null) updateCategoryDir(categoryId, folder);
    } catch (cause) {
      setError(describeError(cause));
    } finally {
      setBrowsing(false);
    }
  }

  async function save(): Promise<void> {
    if (draft === null || settings === null || saving) return;
    setSaving(true);
    setError(null);
    setSaved(null);
    try {
      const maxParallel = integerInRange(
        draft.maxParallel,
        "Parallel downloads",
        1,
        32,
      );
      const maxConnections = integerInRange(
        draft.maxConnections,
        "Connection budget",
        1,
        128,
      );
      const retryAttempts = integerInRange(
        draft.retryMaxAttempts,
        "Retry attempts",
        0,
        10,
      );
      const speedKbps = draft.globalSpeedKbps.trim();
      let globalSpeedLimit: number | null = null;
      if (speedKbps !== "") {
        const speed = integerInRange(speedKbps, "Speed limit", 1, 1_048_576);
        globalSpeedLimit = speed * 1024;
      }
      const defaultDir = draft.defaultDestDir.trim();
      if (defaultDir !== "" && !isAbsoluteFolder(defaultDir)) {
        throw new Error("The default folder must be an absolute path.");
      }

      for (const entries of Object.entries(categoryDirs)) {
        const folder = entries[1];
        if (folder !== undefined && !isAbsoluteFolder(folder)) {
          throw new Error("Every category folder must be an absolute path.");
        }
      }

      const updates: readonly [SettingName, unknown][] = [
        ["max_parallel", maxParallel],
        ["max_connections_total", maxConnections],
        ["global_speed_limit", globalSpeedLimit],
        ["resume_on_start", draft.resumeOnStart],
        ["retry_max_attempts", retryAttempts],
        ["default_dest_dir", defaultDir],
      ];
      let latest = settings;
      for (const [name, value] of updates) {
        latest = await updateSetting(name, value);
      }
      writeCategoryDirs(categoryDirs);
      writeDeepLinkConfirm(confirmBrowserLinks);
      setSettings(latest);
      setDraft(draftFrom(latest));
      setSaved("Settings saved. New transfers use the updated limits.");
    } catch (cause) {
      setError(describeError(cause));
    } finally {
      setSaving(false);
    }
  }

  async function refreshSchedules(): Promise<void> {
    try {
      setSchedules(await listSchedules());
    } catch (cause) {
      setScheduleError(describeError(cause));
    }
  }

  async function addSchedule(): Promise<void> {
    if (creating) return;
    setScheduleError(null);
    setCreating(true);
    try {
      const label = newLabel.trim();
      if (label === "") throw new Error("Give the start window a name.");
      const timestamp = new Date(newStart).getTime();
      if (!Number.isFinite(timestamp)) {
        throw new Error("Choose a valid start date and time.");
      }
      if (newKind === "once" && timestamp <= Date.now()) {
        throw new Error("A one-time window must start in the future.");
      }
      if (newKind === "weekly" && newWeekdays === 0) {
        throw new Error("Choose at least one day for a weekly window.");
      }
      const windowMinutes = integerInRange(newWindow, "Open length", 0, 10_080);
      const input: ScheduleCreateInput = {
        label,
        kind: newKind,
        startAt: timestamp,
        windowMinutes,
        weekdays: newKind === "weekly" ? newWeekdays : 0,
        enabled: newEnabled,
      };
      await createSchedule(input);
      setNewLabel("");
      setNewStart(localInputValue(new Date(Date.now() + 60 * 60 * 1000)));
      setNewWindow("60");
      await refreshSchedules();
    } catch (cause) {
      setScheduleError(describeError(cause));
    } finally {
      setCreating(false);
    }
  }

  async function toggleSchedule(schedule: ScheduleView): Promise<void> {
    setScheduleBusy(schedule.id);
    setScheduleError(null);
    try {
      await setScheduleEnabled(schedule.id, !schedule.enabled);
      await refreshSchedules();
    } catch (cause) {
      setScheduleError(describeError(cause));
    } finally {
      setScheduleBusy(null);
    }
  }

  async function removeSchedule(schedule: ScheduleView): Promise<void> {
    if (
      !window.confirm(
        `Delete “${schedule.label}”? Downloads already queued keep running.`,
      )
    ) {
      return;
    }
    setScheduleBusy(schedule.id);
    setScheduleError(null);
    try {
      await deleteSchedule(schedule.id);
      await refreshSchedules();
    } catch (cause) {
      setScheduleError(describeError(cause));
    } finally {
      setScheduleBusy(null);
    }
  }

  const TabIcon = TABS.find((entry) => entry.id === tab)?.icon ?? Settings2;

  return (
    <DialogWindow
      title="Settings"
      description="Queue limits, transfer behavior, folders, and start windows."
      icon={<Settings2 size={17} strokeWidth={1.9} />}
      onClose={onClose}
      initialWidth={860}
      initialHeight={720}
      minWidth={640}
      minHeight={540}
    >
      <div className="flex min-h-0 flex-1 flex-col">
        <div className="flex min-h-0 flex-1">
          <nav
            aria-label="Settings sections"
            className="flex w-44 shrink-0 flex-col gap-0.5 border-r border-line/80 bg-canvas/25 p-2"
          >
            {TABS.map((entry) => {
              const Icon = entry.icon;
              const active = tab === entry.id;
              return (
                <button
                  key={entry.id}
                  type="button"
                  onClick={() => setTab(entry.id)}
                  aria-current={active ? "page" : undefined}
                  className={cn(
                    "flex h-8 items-center gap-2 rounded-md px-2.5 text-left text-[12px] font-medium transition-colors duration-150 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ink/60",
                    active
                      ? "bg-ink/10 text-ink"
                      : "text-muted hover:bg-ink/[0.04] hover:text-ink",
                  )}
                >
                  <Icon size={14} strokeWidth={1.8} />
                  {entry.label}
                </button>
              );
            })}
          </nav>

          <div className="min-h-0 flex-1 overflow-y-auto px-5 py-5">
            <div className="mb-4 flex items-center gap-2">
              <TabIcon size={15} strokeWidth={1.8} className="text-ink-soft" />
              <h3 className="text-[13px] font-semibold text-ink">
                {TABS.find((entry) => entry.id === tab)?.label}
              </h3>
            </div>

            {loading ? (
              <SettingsSkeleton />
            ) : draft === null ? (
              <div className="rounded-lg border border-ink/60 bg-ink/20 px-4 py-5">
                <p className="text-[12px] font-medium text-ink">
                  Settings could not be loaded.
                </p>
                <p className="mt-1 text-[11px] leading-relaxed text-muted">
                  {error ?? "The host did not return the current settings."}
                </p>
                <Button
                  variant="secondary"
                  size="sm"
                  className="mt-3"
                  onClick={() => {
                    setLoading(true);
                    setError(null);
                    setLoadAttempt((attempt) => attempt + 1);
                  }}
                >
                  Try again
                </Button>
              </div>
            ) : tab === "general" ? (
              <GeneralPanel
                draft={draft}
                onDraft={updateDraft}
                onBrowse={() => void browseDefault()}
                browsing={browsing}
                confirmBrowserLinks={confirmBrowserLinks}
                onConfirmBrowserLinks={setConfirmBrowserLinks}
                theme={theme}
                onTheme={(next) => {
                  setTheme(next);
                  writeTheme(next);
                  applyTheme(next);
                }}
              />
            ) : tab === "folders" ? (
              <FoldersPanel
                categoryDirs={categoryDirs}
                onChange={updateCategoryDir}
                onBrowse={(categoryId, current) =>
                  void browseCategory(categoryId, current)
                }
                browsing={browsing}
              />
            ) : tab === "transfers" ? (
              <TransfersPanel draft={draft} onDraft={updateDraft} />
            ) : (
              <SchedulesPanel
                schedules={schedules}
                busy={scheduleBusy}
                error={scheduleError}
                newLabel={newLabel}
                newKind={newKind}
                newWeekdays={newWeekdays}
                newStart={newStart}
                newWindow={newWindow}
                newEnabled={newEnabled}
                creating={creating}
                onLabel={setNewLabel}
                onKind={setNewKind}
                onWeekdays={setNewWeekdays}
                onStart={setNewStart}
                onWindow={setNewWindow}
                onEnabled={setNewEnabled}
                onCreate={() => void addSchedule()}
                onToggle={(schedule) => void toggleSchedule(schedule)}
                onDelete={(schedule) => void removeSchedule(schedule)}
              />
            )}

            {error !== null && (
              <p
                role="alert"
                className="mt-4 rounded-lg border border-ink/70 bg-ink/35 px-3 py-2.5 text-[11.5px] leading-relaxed text-ink"
              >
                {error}
              </p>
            )}
          </div>
        </div>

        <footer className="flex shrink-0 flex-wrap items-center gap-2 border-t border-line bg-canvas/25 px-4 py-3">
          <p className="min-w-0 flex-1 text-[11px] text-muted">
            {saved !== null ? (
              <span className="inline-flex items-center gap-1.5 text-ink">
                <Check size={12} strokeWidth={2} /> {saved}
              </span>
            ) : dirty ? (
              "Unsaved changes apply to new transfers."
            ) : (
              "Settings are stored on this device."
            )}
          </p>
          <Button variant="ghost" onClick={onClose}>
            Close
          </Button>
          <Button
            onClick={() => void save()}
            disabled={saving || settings === null || !dirty}
          >
            {saving ? (
              <>
                <Loader2 size={13} className="animate-spin" /> Saving
              </>
            ) : (
              "Save changes"
            )}
          </Button>
        </footer>
      </div>
    </DialogWindow>
  );
}

function GeneralPanel({
  draft,
  onDraft,
  onBrowse,
  browsing,
  confirmBrowserLinks,
  onConfirmBrowserLinks,
  theme,
  onTheme,
}: {
  readonly draft: SettingsDraft;
  readonly onDraft: <K extends keyof SettingsDraft>(
    key: K,
    value: SettingsDraft[K],
  ) => void;
  readonly onBrowse: () => void;
  readonly browsing: boolean;
  readonly confirmBrowserLinks: boolean;
  readonly onConfirmBrowserLinks: (checked: boolean) => void;
  readonly theme: ThemeChoice;
  readonly onTheme: (theme: ThemeChoice) => void;
}) {
  return (
    <div className="space-y-5">
      <section>
        <label htmlFor="settings-default-dir" className={LABEL}>
          Default download folder
        </label>
        <div className="flex gap-2">
          <input
            id="settings-default-dir"
            type="text"
            value={draft.defaultDestDir}
            onChange={(event) => onDraft("defaultDestDir", event.target.value)}
            spellCheck={false}
            className={FIELD}
          />
          <Button
            variant="secondary"
            size="sm"
            onClick={onBrowse}
            disabled={browsing}
          >
            <FolderCog size={14} strokeWidth={1.8} />
            {browsing ? "Choosing" : "Browse"}
          </Button>
        </div>
        <p className={HELP}>
          Leave empty to use the operating system&apos;s Downloads folder.
          Category folders can override this.
        </p>
      </section>

      <section>
        <span className={LABEL}>Appearance</span>
        <div
          role="radiogroup"
          aria-label="Appearance"
          className="inline-flex rounded-lg border border-line bg-panel/40 p-0.5"
        >
          {(
            [
              ["system", "System"],
              ["light", "Light"],
              ["dark", "Dark"],
            ] as const
          ).map(([value, label]) => (
            <button
              key={value}
              type="button"
              role="radio"
              aria-checked={theme === value}
              onClick={() => onTheme(value)}
              className={
                theme === value
                  ? "h-7 rounded-[6px] bg-ink px-3 text-[11.5px] font-medium text-canvas"
                  : "h-7 rounded-[6px] px-3 text-[11.5px] font-medium text-muted transition-colors hover:text-ink"
              }
            >
              {label}
            </button>
          ))}
        </div>
        <p className={HELP}>
          System follows the operating system. The extension uses the same
          palette.
        </p>
      </section>

      <section className="rounded-lg border border-line bg-panel/30 p-3.5">
        <div className="flex items-center justify-between gap-4">
          <div>
            <p className="text-[12px] font-medium text-ink">
              Resume queue on launch
            </p>
            <p className="mt-0.5 text-[11px] leading-relaxed text-muted">
              Continue unfinished transfers when RAPIDS starts.
            </p>
          </div>
          <Toggle
            checked={draft.resumeOnStart}
            onChange={(checked) => onDraft("resumeOnStart", checked)}
            label="Resume queue on launch"
          />
        </div>
      </section>

      <section className="rounded-lg border border-line bg-panel/30 p-3.5">
        <div className="flex items-center justify-between gap-4">
          <div>
            <p className="text-[12px] font-medium text-ink">
              Ask before adding links sent from the browser
            </p>
            <p className="mt-0.5 text-[11px] leading-relaxed text-muted">
              Browser handoffs open the Add download sheet for review. Turn this
              off to queue them immediately.
            </p>
          </div>
          <Toggle
            checked={confirmBrowserLinks}
            onChange={onConfirmBrowserLinks}
            label="Ask before adding links sent from the browser"
          />
        </div>
        {!confirmBrowserLinks && (
          <p
            role="status"
            className="mt-2.5 rounded-md border border-ink/25 bg-ink/[0.06] px-2.5 py-2 text-[11px] leading-relaxed text-ink"
          >
            Links opened by web pages or the browser extension will be added
            without showing the confirmation sheet.
          </p>
        )}
      </section>

      <section className="grid gap-4 sm:grid-cols-2">
        <div>
          <label htmlFor="settings-parallel" className={LABEL}>
            Parallel downloads
          </label>
          <input
            id="settings-parallel"
            type="number"
            min="1"
            max="32"
            step="1"
            value={draft.maxParallel}
            onChange={(event) => onDraft("maxParallel", event.target.value)}
            className={FIELD}
          />
          <p className={HELP}>How many files may transfer at once.</p>
        </div>
        <div>
          <label htmlFor="settings-connections" className={LABEL}>
            Connection budget
          </label>
          <input
            id="settings-connections"
            type="number"
            min="1"
            max="128"
            step="1"
            value={draft.maxConnections}
            onChange={(event) => onDraft("maxConnections", event.target.value)}
            className={FIELD}
          />
          <p className={HELP}>Shared across every running transfer.</p>
        </div>
      </section>

      <section>
        <label htmlFor="settings-retries" className={LABEL}>
          Retry attempts
        </label>
        <input
          id="settings-retries"
          type="number"
          min="0"
          max="10"
          step="1"
          value={draft.retryMaxAttempts}
          onChange={(event) => onDraft("retryMaxAttempts", event.target.value)}
          className={`${FIELD} max-w-40`}
        />
        <p className={HELP}>
          Failed transfers retry with a growing delay. Zero disables automatic
          retries.
        </p>
      </section>
    </div>
  );
}

function FoldersPanel({
  categoryDirs,
  onChange,
  onBrowse,
  browsing,
}: {
  readonly categoryDirs: CategoryDirs;
  readonly onChange: (categoryId: keyof CategoryDirs, value: string) => void;
  readonly onBrowse: (categoryId: keyof CategoryDirs, current: string) => void;
  readonly browsing: boolean;
}) {
  return (
    <div className="space-y-3">
      <p className="text-[11.5px] leading-relaxed text-muted">
        Category folders are optional. When set, they take precedence over the
        default folder for new downloads in that category.
      </p>
      <div className="divide-y divide-line/70 rounded-lg border border-line">
        {CATEGORIES.map((category) => {
          const Icon = category.icon;
          const value = categoryDirs[category.id] ?? "";
          return (
            <div
              key={category.id}
              className="flex items-center gap-2 px-3 py-2.5"
            >
              <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-ink/[0.04] text-muted">
                <Icon size={14} strokeWidth={1.8} />
              </span>
              <div className="min-w-0 flex-1">
                <label
                  htmlFor={`category-dir-${category.id}`}
                  className="block text-[11.5px] font-medium text-ink-soft"
                >
                  {category.label}
                </label>
                <input
                  id={`category-dir-${category.id}`}
                  type="text"
                  value={value}
                  spellCheck={false}
                  onChange={(event) =>
                    onChange(category.id, event.target.value)
                  }
                  className="mt-1 h-7 w-full rounded-md border border-line bg-canvas/50 px-2 text-[11px] text-ink outline-none transition-colors focus:border-ink/60 focus:ring-1 focus:ring-ink/20"
                />
              </div>
              <Button
                variant="secondary"
                size="sm"
                onClick={() => onBrowse(category.id, value)}
                disabled={browsing}
                className="mt-4"
              >
                Browse
              </Button>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function TransfersPanel({
  draft,
  onDraft,
}: {
  readonly draft: SettingsDraft;
  readonly onDraft: <K extends keyof SettingsDraft>(
    key: K,
    value: SettingsDraft[K],
  ) => void;
}) {
  return (
    <div className="space-y-5">
      <section>
        <label htmlFor="settings-speed" className={LABEL}>
          Global speed ceiling
        </label>
        <div className="relative max-w-64">
          <input
            id="settings-speed"
            type="number"
            min="1"
            step="1"
            value={draft.globalSpeedKbps}
            onChange={(event) => onDraft("globalSpeedKbps", event.target.value)}
            className={`${FIELD} pr-16`}
          />
          <span className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-[10px] text-muted">
            KiB/s
          </span>
        </div>
        <p className={HELP}>
          Leave empty for no global ceiling. Per-download ceilings can be set in
          Add download.
        </p>
      </section>

      <section className="rounded-lg border border-line bg-panel/30 p-3.5">
        <p className="text-[12px] font-medium text-ink">
          What the queue controls
        </p>
        <p className="mt-1 text-[11.5px] leading-relaxed text-muted">
          Pausing the queue stops new transfers from starting. Transfers already
          in progress keep running until they finish, pause, or are cancelled.
        </p>
      </section>

      <section className="rounded-lg border border-line bg-panel/30 p-3.5">
        <p className="text-[12px] font-medium text-ink">Retry schedule</p>
        <p className="mt-1 text-[11.5px] leading-relaxed text-muted">
          RAPIDS waits five seconds after the first failure, then doubles the
          delay up to five minutes. The retry count is set in General.
        </p>
      </section>
    </div>
  );
}

function SchedulesPanel({
  schedules,
  busy,
  error,
  newLabel,
  newKind,
  newWeekdays,
  newStart,
  newWindow,
  newEnabled,
  creating,
  onLabel,
  onKind,
  onWeekdays,
  onStart,
  onWindow,
  onEnabled,
  onCreate,
  onToggle,
  onDelete,
}: {
  readonly schedules: readonly ScheduleView[];
  readonly busy: number | null;
  readonly error: string | null;
  readonly newLabel: string;
  readonly newKind: ScheduleView["kind"];
  readonly newWeekdays: number;
  readonly newStart: string;
  readonly newWindow: string;
  readonly newEnabled: boolean;
  readonly creating: boolean;
  readonly onLabel: (value: string) => void;
  readonly onKind: (value: ScheduleView["kind"]) => void;
  readonly onWeekdays: (value: number) => void;
  readonly onStart: (value: string) => void;
  readonly onWindow: (value: string) => void;
  readonly onEnabled: (value: boolean) => void;
  readonly onCreate: () => void;
  readonly onToggle: (schedule: ScheduleView) => void;
  readonly onDelete: (schedule: ScheduleView) => void;
}) {
  return (
    <div className="space-y-5">
      <section>
        <div className="mb-2 flex items-center justify-between gap-3">
          <div>
            <p className="text-[12px] font-medium text-ink">Start windows</p>
            <p className="mt-0.5 text-[11px] text-muted">
              Assign a window to a download from its Transfer options.
            </p>
          </div>
        </div>
        {schedules.length === 0 ? (
          <p className="rounded-lg border border-dashed border-line px-3 py-4 text-center text-[11.5px] text-muted">
            No start windows yet. Add one below to hold new downloads until a
            chosen time.
          </p>
        ) : (
          <ul className="divide-y divide-line/70 overflow-hidden rounded-lg border border-line">
            {schedules.map((schedule) => (
              <li
                key={schedule.id}
                className="flex items-center gap-3 px-3 py-2.5"
              >
                <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-ink/10 text-ink">
                  <CalendarClock size={14} strokeWidth={1.8} />
                </span>
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[12px] font-medium text-ink">
                    {schedule.label}
                  </p>
                  <p className="mt-0.5 truncate text-[10.5px] text-muted">
                    {scheduleKindLabel(schedule.kind)}
                    {schedule.kind === "weekly"
                      ? ` (${weekdayNames(schedule.weekdays)})`
                      : ""}{" "}
                    · {scheduleWhen(schedule)} ·{" "}
                    {scheduleWindow(schedule.windowMinutes)}
                  </p>
                </div>
                <Toggle
                  checked={schedule.enabled}
                  onChange={() => onToggle(schedule)}
                  label={`${schedule.enabled ? "Disable" : "Enable"} ${schedule.label}`}
                  disabled={busy === schedule.id}
                />
                <IconButton
                  label={`Delete ${schedule.label}`}
                  onClick={() => onDelete(schedule)}
                  disabled={busy === schedule.id}
                  className="h-7 w-7 text-muted hover:text-ink"
                >
                  {busy === schedule.id ? (
                    <Loader2 size={13} className="animate-spin" />
                  ) : (
                    <Trash2 size={13} strokeWidth={1.8} />
                  )}
                </IconButton>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="rounded-lg border border-line bg-panel/30 p-3.5">
        <p className="mb-3 flex items-center gap-1.5 text-[12px] font-medium text-ink">
          <Plus size={13} strokeWidth={2} /> Add a start window
        </p>
        <div className="grid gap-3 sm:grid-cols-2">
          <div className="sm:col-span-2">
            <label htmlFor="schedule-label" className={LABEL}>
              Name
            </label>
            <input
              id="schedule-label"
              type="text"
              value={newLabel}
              onChange={(event) => onLabel(event.target.value)}
              className={FIELD}
            />
          </div>
          <div>
            <label htmlFor="schedule-kind" className={LABEL}>
              Repeats
            </label>
            <select
              id="schedule-kind"
              value={newKind}
              onChange={(event) => {
                const kind = event.target.value;
                onKind(kind === "daily" || kind === "weekly" ? kind : "once");
              }}
              className={FIELD}
            >
              <option value="once">Once</option>
              <option value="daily">Every day</option>
              <option value="weekly">Selected days</option>
            </select>
          </div>
          {newKind === "weekly" && (
            <div className="sm:col-span-2">
              <span className={LABEL}>Days</span>
              <div
                role="group"
                aria-label="Days of the week"
                className="flex flex-wrap gap-1"
              >
                {WEEKDAYS.map((day) => {
                  const selected = (newWeekdays & day.bit) !== 0;
                  return (
                    <button
                      key={day.bit}
                      type="button"
                      aria-pressed={selected}
                      onClick={() => onWeekdays(newWeekdays ^ day.bit)}
                      className={cn(
                        "h-8 min-w-11 rounded-md border px-2 text-[11px] font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/50",
                        selected
                          ? "border-ink/50 bg-ink/10 text-ink"
                          : "border-line bg-canvas/45 text-muted hover:border-line-strong hover:text-ink",
                      )}
                    >
                      {day.label}
                    </button>
                  );
                })}
              </div>
              <p className={HELP}>
                The opening time repeats on each selected day. Choose at least
                one.
              </p>
            </div>
          )}
          <div>
            <label htmlFor="schedule-start" className={LABEL}>
              {newKind === "weekly" ? "Time of day" : "Opens at"}
            </label>
            <input
              id="schedule-start"
              type="datetime-local"
              value={newStart}
              onChange={(event) => onStart(event.target.value)}
              className={FIELD}
            />
          </div>
          <div>
            <label htmlFor="schedule-window" className={LABEL}>
              Stays open
            </label>
            <div className="relative">
              <input
                id="schedule-window"
                type="number"
                min="0"
                max="10080"
                step="1"
                value={newWindow}
                onChange={(event) => onWindow(event.target.value)}
                className={`${FIELD} pr-16`}
              />
              <span className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-[10px] text-muted">
                minutes
              </span>
            </div>
            <p className={HELP}>
              Zero keeps the window open until its next start.
            </p>
          </div>
          <div className="flex items-end justify-between gap-3 pb-1">
            <div>
              <p className="text-[12px] font-medium text-ink-soft">Enabled</p>
              <p className="mt-0.5 text-[11px] text-muted">
                Ready for new downloads.
              </p>
            </div>
            <Toggle
              checked={newEnabled}
              onChange={onEnabled}
              label="Enable new start window"
            />
          </div>
        </div>
        <div className="mt-3 flex items-center justify-end">
          <Button size="sm" onClick={onCreate} disabled={creating}>
            {creating ? (
              <>
                <Loader2 size={12} className="animate-spin" /> Adding
              </>
            ) : (
              <>
                <Plus size={12} strokeWidth={2} /> Add window
              </>
            )}
          </Button>
        </div>
        {error !== null && (
          <p role="alert" className="mt-2 text-[11px] leading-relaxed text-ink">
            {error}
          </p>
        )}
      </section>
    </div>
  );
}
