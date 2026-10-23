import {
  AlertTriangle,
  ArrowDownToLine,
  Check,
  ChevronDown,
  ClipboardPaste,
  FolderOpen,
  Globe2,
  Link2,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type DragEvent,
  type FormEvent,
  type KeyboardEvent,
} from "react";
import { CATEGORIES, categoryFor } from "../../config/categories";
import {
  createAddDraft,
  toAddRequest,
  type AddDraft,
} from "../../lib/addDraft";
import type { DeepLinkRequest } from "../../lib/deeplink";
import {
  inspectUrl,
  normalizeClipboardUrl,
  suggestedCategory,
  urlKey,
  type UrlFileInfo,
} from "../../lib/fileMeta";
import { formatBytes } from "../../lib/format";
import {
  addTask,
  describeError,
  fetchDownloadDefaultDir,
  getSettings,
  listSchedules,
} from "../../lib/ipc";
import { pickFolder, readClipboardText } from "../../lib/native";
import {
  destinationFor,
  readCategoryDirs,
  type CategoryDirs,
} from "../../lib/preferences";
import { cn } from "../../lib/cn";
import {
  useRefreshTasks,
  useStoreDispatch,
  useStoreState,
} from "../../state/StoreProvider";
import type { ScheduleView, Settings } from "../../types";
import { Button } from "../ui/Button";
import { DialogWindow } from "../ui/DialogWindow";

const FIELD =
  "h-10 w-full rounded-lg border border-line bg-canvas/55 px-3 text-[13px] text-ink outline-none transition-[border-color,background-color,box-shadow] duration-150 ease-out focus:border-ink/60 focus:bg-canvas focus:ring-2 focus:ring-ink/15";
const LABEL = "mb-1.5 block text-[12px] font-medium text-ink-soft";
const HELP = "mt-1.5 text-[11px] leading-relaxed text-muted";

function initialDraft(url: string | undefined): AddDraft {
  const info = inspectUrl(url ?? "");
  return {
    ...createAddDraft(),
    url: url?.trim() ?? "",
    category: suggestedCategory(info),
  };
}

function kindLabel(info: UrlFileInfo): string {
  if (!info.valid) return "Waiting for a link";
  if (info.isStreaming) return "Streaming manifest";
  if (info.isPageLike) return "Web page";
  if (info.fileName === null) return "No file name in link";
  if (info.extension === null) return "Unverified file";
  return "Direct file";
}

function kindTone(info: UrlFileInfo): string {
  if (!info.valid) return "bg-elevated text-muted";
  if (
    info.isStreaming ||
    info.isPageLike ||
    info.fileName === null ||
    info.extension === null
  ) {
    return "bg-ink/10 text-ink";
  }
  return "bg-ink/10 text-ink";
}

function taskStatusLabel(status: string): string {
  if (status === "error") return "failed";
  if (status === "scheduled") return "scheduled";
  return status;
}

function handoffKindLabel(kind: DeepLinkRequest["kind"]): string {
  if (kind === "video") return "Video";
  if (kind === "audio") return "Audio";
  return "File";
}

function BrowserHandoffBanner({
  handoff,
}: {
  readonly handoff: DeepLinkRequest;
}) {
  const hint = [
    handoff.name,
    handoff.size === null ? null : formatBytes(handoff.size),
  ]
    .filter((value): value is string => value !== null && value !== "")
    .join(" · ");

  return (
    <section className="rounded-xl border border-ink/25 bg-ink/[0.055] px-3.5 py-3">
      <div className="flex items-start gap-3">
        <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg border border-ink/25 bg-ink/10 text-ink">
          <Globe2 size={16} strokeWidth={1.8} />
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="inline-flex items-center gap-1 rounded-full bg-ink/15 px-2 py-0.5 text-[10px] font-semibold text-ink">
              <Globe2 size={10} strokeWidth={2} />
              Sent from browser
            </span>
            <span className="rounded-full bg-ink/[0.05] px-2 py-0.5 text-[10px] font-medium text-muted">
              {handoffKindLabel(handoff.kind)}
            </span>
          </div>
          {hint !== "" && (
            <p
              className="mt-1.5 truncate text-[12px] text-ink-soft"
              title={hint}
            >
              Browser suggests: {hint}
            </p>
          )}
          {handoff.sourcePage !== null && (
            <p
              className="mt-0.5 truncate text-[11px] text-muted"
              title={handoff.sourcePage}
            >
              From {handoff.sourcePage}
            </p>
          )}
          <p className="mt-2 text-[11px] leading-relaxed text-muted">
            Review the destination and transfer options, then add the download.
            RAPIDS confirms the file name when the transfer starts.
          </p>
        </div>
      </div>
    </section>
  );
}

export function AddDownloadDialog({
  onClose,
  initialUrl,
  handoff,
}: {
  readonly onClose: () => void;
  readonly initialUrl?: string;
  readonly handoff?: DeepLinkRequest;
}) {
  const inFlight = useRef(false);
  const [draft, setDraft] = useState<AddDraft>(() =>
    initialDraft(initialUrl ?? handoff?.url),
  );
  const [defaults, setDefaults] = useState<Settings | null>(null);
  const [systemDownloads, setSystemDownloads] = useState("");
  const [schedules, setSchedules] = useState<readonly ScheduleView[]>([]);
  const [categoryDirs] = useState<CategoryDirs>(readCategoryDirs);
  const [categoryTouched, setCategoryTouched] = useState(false);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [browsing, setBrowsing] = useState(false);
  const [pasting, setPasting] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [allowDuplicate, setAllowDuplicate] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const dispatch = useStoreDispatch();
  const refresh = useRefreshTasks();
  const { tasks } = useStoreState();

  const info = useMemo(() => inspectUrl(draft.url), [draft.url]);
  const category = categoryFor(draft.category);
  const CategoryIcon = category.icon;
  const categoryDest = destinationFor(draft.category, categoryDirs);
  const defaultDest = defaults?.defaultDestDir.trim() ?? "";
  const effectiveDest =
    draft.destDir.trim() || categoryDest || defaultDest || systemDownloads;
  const destinationSource = draft.destDir.trim()
    ? "Custom path"
    : categoryDest !== null
      ? "Category folder"
      : defaultDest !== ""
        ? "Default folder"
        : "System Downloads";
  const duplicate = useMemo(() => {
    if (!info.valid || draft.url.trim() === "") return null;
    const key = urlKey(draft.url);
    return (
      tasks.find(
        (task) =>
          urlKey(task.url) === key ||
          (task.finalUrl !== null && urlKey(task.finalUrl) === key),
      ) ?? null
    );
  }, [draft.url, info.valid, tasks]);
  const selectedSchedule =
    schedules.find((schedule) => String(schedule.id) === draft.scheduleId) ??
    null;

  useEffect(() => {
    let active = true;
    void Promise.allSettled([
      getSettings(),
      fetchDownloadDefaultDir(),
      listSchedules(),
    ]).then(([settingsResult, dirResult, schedulesResult]) => {
      if (!active) return;
      if (settingsResult.status === "fulfilled")
        setDefaults(settingsResult.value);
      if (dirResult.status === "fulfilled") setSystemDownloads(dirResult.value);
      if (schedulesResult.status === "fulfilled") {
        setSchedules(schedulesResult.value);
      }
    });
    return () => {
      active = false;
    };
  }, []);

  function change<K extends keyof AddDraft>(
    field: K,
    value: AddDraft[K],
  ): void {
    setDraft((current) => ({ ...current, [field]: value }));
    setError(null);
  }

  function changeUrl(value: string): void {
    setDraft((current) => ({
      ...current,
      url: value,
      category: categoryTouched
        ? current.category
        : suggestedCategory(inspectUrl(value)),
    }));
    setAllowDuplicate(false);
    setError(null);
  }

  function chooseCategory(next: AddDraft["category"]): void {
    setCategoryTouched(true);
    setDraft((current) => ({ ...current, category: next }));
    setError(null);
  }

  function requestClose(): void {
    if (inFlight.current) {
      setError(
        "The download request is being added. Wait for its result before closing.",
      );
      return;
    }
    onClose();
  }

  async function browse(): Promise<void> {
    if (browsing) return;
    setBrowsing(true);
    setError(null);
    try {
      const folder = await pickFolder(
        effectiveDest,
        "Choose a download folder",
      );
      if (folder !== null) {
        setDraft((current) => ({ ...current, destDir: folder }));
      }
    } catch (cause) {
      setError(describeError(cause));
    } finally {
      setBrowsing(false);
    }
  }

  async function paste(): Promise<void> {
    if (pasting) return;
    setPasting(true);
    setError(null);
    try {
      const clipboard = await readClipboardText();
      const url = normalizeClipboardUrl(clipboard);
      if (url === null) {
        setError("The clipboard does not contain an HTTP or HTTPS link.");
        return;
      }
      changeUrl(url);
    } catch (cause) {
      setError(describeError(cause));
    } finally {
      setPasting(false);
    }
  }

  function droppedUrl(event: DragEvent<HTMLFormElement>): void {
    event.preventDefault();
    setDragging(false);
    const raw =
      event.dataTransfer.getData("text/uri-list") ||
      event.dataTransfer.getData("text/plain");
    const url = normalizeClipboardUrl(raw);
    if (url === null) {
      setError("Drop an HTTP or HTTPS link here.");
      return;
    }
    changeUrl(url);
  }

  async function submit(): Promise<void> {
    if (inFlight.current) {
      setError("The download is already being added.");
      return;
    }
    if (!info.valid) {
      setError(info.error ?? "Enter a valid HTTP or HTTPS link.");
      return;
    }
    if (info.fileName === null) {
      setError(
        "This link does not name a file. Use a direct file link, or open the page with Inspect a media page.",
      );
      return;
    }
    if (info.isStreaming) {
      setError(
        "This is a streaming manifest. RAPIDS only saves direct files; open the page with Inspect a media page for media review.",
      );
      return;
    }
    if (duplicate !== null && !allowDuplicate) {
      setError(
        "This link is already in your library. Tick Add another copy to continue.",
      );
      return;
    }

    const requestDraft: AddDraft = {
      ...draft,
      destDir:
        draft.destDir.trim() !== "" ? draft.destDir : (categoryDest ?? ""),
    };
    let request;
    try {
      request = toAddRequest(requestDraft);
    } catch (cause) {
      setError(describeError(cause));
      return;
    }

    inFlight.current = true;
    setSubmitting(true);
    setError(null);
    try {
      await addTask(request);
    } catch (cause) {
      inFlight.current = false;
      setSubmitting(false);
      refresh();
      setError(
        `${describeError(cause)} Check the task list before trying again.`,
      );
      return;
    }
    inFlight.current = false;
    dispatch({ type: "navigate", viewId: "all" });
    dispatch({ type: "search/set", query: "" });
    refresh();
    onClose();
  }

  function onSubmit(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    void submit();
  }

  function onKeyDown(event: KeyboardEvent<HTMLFormElement>): void {
    if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
      event.preventDefault();
      void submit();
    }
  }

  const blockedReason = !info.valid
    ? "Enter a complete HTTP or HTTPS link."
    : info.fileName === null
      ? "This link does not name a file. Use a direct file link."
      : info.isStreaming
        ? "Streaming manifests cannot be added as direct files."
        : duplicate !== null && !allowDuplicate
          ? "Tick Add another copy to add this link again."
          : null;

  return (
    <DialogWindow
      title="Add download"
      description="Queue a direct HTTP or HTTPS file. The transfer starts when a queue slot opens."
      icon={<ArrowDownToLine size={17} strokeWidth={1.9} />}
      onClose={requestClose}
      initialWidth={900}
      initialHeight={700}
      minWidth={640}
      minHeight={540}
    >
      <form
        onSubmit={onSubmit}
        onKeyDown={onKeyDown}
        onDragOver={(event) => {
          event.preventDefault();
          setDragging(true);
        }}
        onDragLeave={() => setDragging(false)}
        onDrop={droppedUrl}
        noValidate
        aria-busy={submitting}
        className={cn(
          "flex min-h-0 flex-1 flex-col",
          dragging && "ring-2 ring-inset ring-ink/70",
        )}
      >
        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-5">
          <div className="mx-auto flex w-full max-w-[54rem] flex-col gap-5">
            {handoff !== undefined && (
              <BrowserHandoffBanner handoff={handoff} />
            )}
            <section>
              <div className="mb-1.5 flex items-center justify-between gap-3">
                <label htmlFor="download-url" className={LABEL}>
                  Download link
                </label>
                <span className="inline-flex items-center gap-1 text-[11px] text-muted">
                  <Link2 size={12} strokeWidth={1.8} />
                  HTTP or HTTPS
                </span>
              </div>
              <div className="relative">
                <Link2
                  size={15}
                  strokeWidth={1.8}
                  className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted"
                />
                <input
                  id="download-url"
                  data-autofocus
                  type="url"
                  inputMode="url"
                  spellCheck={false}
                  autoComplete="off"
                  value={draft.url}
                  onChange={(event) => changeUrl(event.target.value)}
                  aria-required="true"
                  aria-describedby="download-url-help"
                  className={`${FIELD} pl-9 pr-20`}
                />
                {draft.url.length === 0 && (
                  <span
                    aria-hidden="true"
                    className="pointer-events-none absolute left-9 right-20 top-1/2 -translate-y-1/2 truncate text-[12px] text-faint"
                  >
                    Paste a direct file link
                  </span>
                )}
                <button
                  type="button"
                  onClick={() => void paste()}
                  disabled={pasting}
                  className="absolute right-1 top-1 inline-flex h-8 items-center gap-1 rounded-md px-2 text-[11px] font-medium text-muted transition-colors hover:bg-ink/[0.06] hover:text-ink focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ink/70 disabled:opacity-50"
                >
                  <ClipboardPaste size={13} strokeWidth={1.8} />
                  {pasting ? "Reading" : "Paste"}
                </button>
              </div>
              <p id="download-url-help" className={HELP}>
                Paste or drop a link. RAPIDS saves the response exactly as the
                server sends it.
              </p>
            </section>

            <section
              aria-live="polite"
              className={cn(
                "rounded-xl border p-3.5",
                info.valid
                  ? "border-line bg-canvas/45"
                  : "border-line/70 bg-canvas/25",
              )}
            >
              {draft.url.trim() === "" ? (
                <div className="flex items-center gap-3 text-[12px] text-muted">
                  <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg border border-line bg-panel text-muted">
                    <Sparkles size={15} strokeWidth={1.7} />
                  </div>
                  <div>
                    <p className="font-medium text-muted">
                      File details appear here
                    </p>
                    <p className="mt-0.5 text-[11px] text-muted">
                      Name, type, and destination are read from the link before
                      it is queued.
                    </p>
                  </div>
                </div>
              ) : !info.valid ? (
                <div className="flex items-start gap-3 text-[12px] text-ink">
                  <AlertTriangle
                    size={15}
                    className="mt-0.5 shrink-0"
                    strokeWidth={1.8}
                  />
                  <p>{info.error}</p>
                </div>
              ) : (
                <div className="flex flex-col gap-3">
                  <div className="flex items-start gap-3">
                    <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg border border-line bg-panel text-ink">
                      <CategoryIcon size={18} strokeWidth={1.7} />
                    </div>
                    <div className="min-w-0 flex-1">
                      <div className="flex flex-wrap items-center gap-2">
                        <p className="truncate text-[13px] font-medium text-ink">
                          {info.fileName ?? "No file name in link"}
                        </p>
                        <span
                          className={cn(
                            "inline-flex shrink-0 items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold",
                            kindTone(info),
                          )}
                        >
                          {info.isStreaming ||
                          info.isPageLike ||
                          info.fileName === null ||
                          info.extension === null ? (
                            <AlertTriangle size={10} strokeWidth={2} />
                          ) : (
                            <Check size={10} strokeWidth={2.4} />
                          )}
                          {kindLabel(info)}
                        </span>
                      </div>
                      <div className="mt-1 flex flex-wrap gap-x-4 gap-y-1 text-[11px] text-muted">
                        <span>{info.typeLabel}</span>
                        <span className="truncate" title={info.host}>
                          {info.host}
                        </span>
                        <span>{category.label}</span>
                      </div>
                    </div>
                  </div>
                  <div className="grid gap-2 rounded-lg border border-line/70 bg-panel/40 px-3 py-2.5 sm:grid-cols-2">
                    <div className="min-w-0">
                      <p className="text-[10px] font-medium uppercase tracking-[0.08em] text-muted">
                        Destination
                      </p>
                      <p
                        className="mt-0.5 truncate text-[11px] text-ink-soft"
                        title={
                          effectiveDest || "Using the system Downloads folder"
                        }
                      >
                        {effectiveDest || "System Downloads folder"}
                      </p>
                    </div>
                    <div className="min-w-0">
                      <p className="text-[10px] font-medium uppercase tracking-[0.08em] text-muted">
                        Source
                      </p>
                      <p className="mt-0.5 truncate text-[11px] text-ink-soft">
                        {destinationSource}
                      </p>
                    </div>
                  </div>
                  {(info.isStreaming ||
                    info.isPageLike ||
                    info.extension === null) && (
                    <p className="rounded-lg border border-ink/20 bg-ink/[0.06] px-3 py-2 text-[11px] leading-relaxed text-ink">
                      {info.isStreaming
                        ? "This link points to a playlist, not one file. RAPIDS cannot merge streaming segments from a direct link; use Inspect a media page to review media the site exposes."
                        : info.isPageLike
                          ? "This looks like a web page. If it is not a direct file, use Inspect a media page instead; RAPIDS will not extract protected or embedded media."
                          : "This link has no file extension. RAPIDS will use the path name; the server may provide a better name when the transfer starts."}
                    </p>
                  )}
                  {duplicate !== null && (
                    <div className="rounded-lg border border-ink/25 bg-ink/[0.06] px-3.5 py-3 text-[12px] text-ink">
                      <div className="flex items-start gap-2">
                        <AlertTriangle
                          size={14}
                          className="mt-0.5 shrink-0"
                          strokeWidth={1.8}
                        />
                        <div className="min-w-0">
                          <p className="font-medium">Already in your library</p>
                          <p
                            className="mt-0.5 truncate text-[11px] text-ink"
                            title={duplicate.destPath}
                          >
                            {duplicate.fileName} -{" "}
                            {taskStatusLabel(duplicate.status)}
                          </p>
                          <label className="mt-2 flex cursor-pointer items-center gap-2 text-[11px] text-ink">
                            <input
                              type="checkbox"
                              checked={allowDuplicate}
                              onChange={(event) => {
                                setAllowDuplicate(event.target.checked);
                                setError(null);
                              }}
                              className="h-3.5 w-3.5 rounded border-ink/40 bg-transparent accent-ink"
                            />
                            Add another copy
                          </label>
                        </div>
                      </div>
                    </div>
                  )}
                </div>
              )}
            </section>

            <section>
              <div className="mb-1.5 flex items-center justify-between gap-3">
                <label htmlFor="download-destination" className={LABEL}>
                  Save to
                </label>
              </div>
              <div className="flex gap-2">
                <div className="relative min-w-0 flex-1">
                  <FolderOpen
                    size={15}
                    strokeWidth={1.8}
                    className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted"
                  />
                  <input
                    id="download-destination"
                    type="text"
                    spellCheck={false}
                    value={draft.destDir}
                    title={effectiveDest || "System Downloads folder"}
                    onChange={(event) => change("destDir", event.target.value)}
                    aria-describedby="download-destination-help"
                    className={`${FIELD} pl-9`}
                  />
                </div>
                <Button
                  variant="secondary"
                  onClick={() => void browse()}
                  disabled={browsing}
                  className="h-10 px-3.5"
                >
                  <FolderOpen size={14} strokeWidth={1.8} />
                  {browsing ? "Choosing" : "Browse"}
                </Button>
              </div>
              <p id="download-destination-help" className={HELP}>
                Leave empty to use the default destination. Browse or type an
                absolute path to override it for this download.
              </p>
            </section>

            <section>
              <div className="mb-2 flex items-center justify-between gap-3">
                <div>
                  <p className={LABEL}>Category</p>
                  <p className="-mt-1 text-[11px] text-muted">
                    Chosen automatically from the file type. Change it if
                    needed.
                  </p>
                </div>
                {categoryTouched && info.valid ? (
                  <button
                    type="button"
                    onClick={() => {
                      setCategoryTouched(false);
                      setDraft((current) => ({
                        ...current,
                        category: suggestedCategory(info),
                      }));
                    }}
                    className="inline-flex shrink-0 items-center gap-1 rounded px-1 py-0.5 text-[11px] font-medium text-ink-soft transition-colors hover:bg-ink/10 hover:text-ink focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ink/60"
                  >
                    <Sparkles size={12} strokeWidth={1.8} />
                    Use auto
                  </button>
                ) : (
                  <span className="inline-flex shrink-0 items-center gap-1 text-[11px] text-ink-soft">
                    <Sparkles size={12} strokeWidth={1.8} />
                    Auto
                  </span>
                )}
              </div>
              <div
                role="radiogroup"
                aria-label="Download category"
                className="grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-4"
              >
                {CATEGORIES.map((entry) => {
                  const Icon = entry.icon;
                  const selected = entry.id === draft.category;
                  return (
                    <button
                      key={entry.id}
                      type="button"
                      role="radio"
                      aria-checked={selected}
                      onClick={() => chooseCategory(entry.id)}
                      className={cn(
                        "group flex min-h-[3.5rem] items-center gap-2.5 rounded-lg border px-2.5 py-2 text-left transition-colors duration-150 ease-out focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/45",
                        selected
                          ? "border-ink/50 bg-ink/[0.08] text-ink"
                          : "border-line bg-panel/40 text-muted hover:border-line-strong hover:bg-ink/[0.03] hover:text-ink",
                      )}
                    >
                      <span
                        className={cn(
                          "flex h-7 w-7 shrink-0 items-center justify-center rounded-md",
                          selected
                            ? "bg-ink/15 text-ink"
                            : "bg-ink/[0.04] text-muted group-hover:text-ink-soft",
                        )}
                      >
                        <Icon size={14} strokeWidth={1.8} />
                      </span>
                      <span className="min-w-0">
                        <span className="block truncate text-[12px] font-medium">
                          {entry.label}
                        </span>
                        <span className="mt-0.5 block truncate text-[10px] text-muted">
                          {entry.description}
                        </span>
                      </span>
                    </button>
                  );
                })}
              </div>
            </section>

            <section className="overflow-hidden rounded-lg border border-line">
              <button
                type="button"
                aria-expanded={advancedOpen}
                onClick={() => setAdvancedOpen((open) => !open)}
                className="flex w-full items-center gap-2 bg-panel/30 px-3.5 py-3 text-left transition-colors hover:bg-ink/[0.025] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ink/40"
              >
                <ShieldCheck
                  size={14}
                  strokeWidth={1.8}
                  className="text-muted"
                />
                <span className="flex-1 text-[12px] font-medium text-ink-soft">
                  Transfer options
                </span>
                <span className="text-[10px] text-muted">
                  Connections, speed, schedule, checksum
                </span>
                <ChevronDown
                  size={14}
                  strokeWidth={1.8}
                  className={cn(
                    "text-muted transition-transform duration-150",
                    advancedOpen && "rotate-180",
                  )}
                />
              </button>
              {advancedOpen && (
                <div className="grid gap-3 border-t border-line/80 px-3.5 py-4 sm:grid-cols-2">
                  <div>
                    <label htmlFor="download-priority" className={LABEL}>
                      Priority
                    </label>
                    <input
                      id="download-priority"
                      type="number"
                      step="1"
                      value={draft.priority}
                      onChange={(event) =>
                        change("priority", event.target.value)
                      }
                      className={FIELD}
                    />
                    <p className={HELP}>Higher values start first.</p>
                  </div>
                  <div>
                    <label htmlFor="download-connections" className={LABEL}>
                      Connections
                    </label>
                    <input
                      id="download-connections"
                      type="number"
                      min="1"
                      max="32"
                      step="1"
                      value={draft.connections}
                      onChange={(event) =>
                        change("connections", event.target.value)
                      }
                      aria-describedby="download-connections-help"
                      className={FIELD}
                    />
                    <p id="download-connections-help" className={HELP}>
                      Leave blank to let the engine choose.
                    </p>
                  </div>
                  <div>
                    <label htmlFor="download-speed" className={LABEL}>
                      Speed ceiling
                    </label>
                    <div className="relative">
                      <input
                        id="download-speed"
                        type="number"
                        min="1"
                        step="1"
                        value={draft.speedLimitBps}
                        onChange={(event) =>
                          change("speedLimitBps", event.target.value)
                        }
                        className={`${FIELD} pr-14`}
                      />
                      <span className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-[10px] text-muted">
                        B/s
                      </span>
                    </div>
                  </div>
                  <div>
                    <label htmlFor="download-schedule" className={LABEL}>
                      Start window
                    </label>
                    <select
                      id="download-schedule"
                      value={draft.scheduleId}
                      onChange={(event) =>
                        change("scheduleId", event.target.value)
                      }
                      className={FIELD}
                    >
                      <option value="">Start immediately</option>
                      {schedules.map((schedule) => (
                        <option key={schedule.id} value={String(schedule.id)}>
                          {schedule.label} ({schedule.kind}
                          {schedule.enabled ? "" : ", disabled"})
                        </option>
                      ))}
                    </select>
                    <p className={HELP}>
                      {schedules.length === 0
                        ? "Add a start window in Settings to pause new transfers until a chosen time."
                        : selectedSchedule !== null && !selectedSchedule.enabled
                          ? "This start window is disabled. Enable it in Settings before the download can start."
                          : "The download waits for this window before entering the queue."}
                    </p>
                  </div>
                  <div className="sm:col-span-2">
                    <label htmlFor="download-sha256" className={LABEL}>
                      Expected SHA-256
                    </label>
                    <input
                      id="download-sha256"
                      type="text"
                      spellCheck={false}
                      value={draft.expectedSha256}
                      onChange={(event) =>
                        change("expectedSha256", event.target.value)
                      }
                      aria-describedby="download-sha256-help"
                      className={`${FIELD} font-mono text-[12px]`}
                    />
                    <p id="download-sha256-help" className={HELP}>
                      Optional. Verify the received file against a 64-character
                      checksum.
                    </p>
                  </div>
                  <p className="rounded-lg border border-line/70 bg-canvas/40 px-3 py-2 text-[11px] leading-relaxed text-muted sm:col-span-2">
                    Per-download sign-in, browser cookies, and streaming
                    extraction are not available for direct transfers. Use
                    Inspect a media page for media the browser can expose.
                  </p>
                </div>
              )}
            </section>
          </div>
        </div>

        <div className="flex shrink-0 flex-col border-t border-line bg-canvas/25 px-5 py-3">
          {error !== null && (
            <p
              role="alert"
              className="mb-2 rounded-lg border border-ink/70 bg-ink/35 px-3 py-2 text-[11.5px] leading-relaxed text-ink"
            >
              {error}
            </p>
          )}
          <div className="flex flex-wrap items-center gap-2">
            <p className="min-w-0 flex-1 text-[11px] text-muted">
              <kbd className="rounded border border-line bg-panel px-1.5 py-0.5 font-sans text-[10px] text-muted">
                Ctrl
              </kbd>{" "}
              +{" "}
              <kbd className="rounded border border-line bg-panel px-1.5 py-0.5 font-sans text-[10px] text-muted">
                Enter
              </kbd>{" "}
              to add -{" "}
              <kbd className="rounded border border-line bg-panel px-1.5 py-0.5 font-sans text-[10px] text-muted">
                Esc
              </kbd>{" "}
              to close
            </p>
            <Button
              variant="ghost"
              onClick={requestClose}
              disabled={submitting}
            >
              Cancel
            </Button>
            <Button
              type="submit"
              disabled={submitting || blockedReason !== null}
              title={blockedReason ?? undefined}
            >
              {submitting
                ? "Adding..."
                : duplicate !== null && allowDuplicate
                  ? "Add another copy"
                  : "Add download"}
            </Button>
          </div>
        </div>
      </form>
    </DialogWindow>
  );
}
