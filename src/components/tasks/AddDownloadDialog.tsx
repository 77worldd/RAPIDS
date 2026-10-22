import { useEffect, useRef, useState, type FormEvent } from "react";
import { CATEGORIES } from "../../config/categories";
import { toAddRequest, createAddDraft, type AddDraft } from "../../lib/addDraft";
import { addTask, describeError } from "../../lib/ipc";
import { useRefreshTasks, useStoreDispatch } from "../../state/StoreProvider";
import { Button } from "../ui/Button";

const FIELD =
  "h-9 w-full rounded-md border border-zinc-700 bg-zinc-900 px-3 text-[13px] text-zinc-100 outline-none transition-colors duration-150 focus:border-emerald-500";
const LABEL = "mb-1.5 block text-[12px] font-medium text-zinc-300";

export function AddDownloadDialog({
  onClose,
  initialUrl,
}: {
  readonly onClose: () => void;
  readonly initialUrl?: string;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const inFlight = useRef(false);
  const [draft, setDraft] = useState<AddDraft>(() => ({
    ...createAddDraft(),
    url: initialUrl ?? "",
  }));
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const dispatch = useStoreDispatch();
  const refresh = useRefreshTasks();

  useEffect(() => {
    const element = dialog.current;
    if (element !== null && !element.open) element.showModal();
    return () => {
      if (element?.open) element.close();
    };
  }, []);

  function change<K extends keyof AddDraft>(field: K, value: AddDraft[K]): void {
    setDraft((current) => ({ ...current, [field]: value }));
    setError(null);
  }

  function requestClose(): void {
    if (inFlight.current) {
      setError("The download request is being added. Wait for its result before closing.");
      return;
    }
    onClose();
  }

  async function submit(): Promise<void> {
    if (inFlight.current) {
      setError("The download is already being added.");
      return;
    }
    let request;
    try {
      request = toAddRequest(draft);
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
      // A failure after insertion may leave a real queued task. Reload the
      // authoritative list before the person decides whether to try again.
      refresh();
      setError(`${describeError(cause)} Check the task list before trying again.`);
      return;
    }
    inFlight.current = false;
    // The command reply can describe the queued insert while the manager has
    // already started it. Fetch the current host state instead of inserting a
    // stale row from the reply or inventing a client-side transition.
    dispatch({ type: "navigate", viewId: "all" });
    dispatch({ type: "search/set", query: "" });
    refresh();
    onClose();
  }

  function onSubmit(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    void submit();
  }

  return (
    <dialog
      ref={dialog}
      aria-labelledby="add-download-title"
      aria-describedby="add-download-description"
      onCancel={(event) => {
        event.preventDefault();
        requestClose();
      }}
      className="w-[min(34rem,calc(100vw-2rem))] rounded-xl border border-zinc-700 bg-zinc-950 p-0 text-zinc-100 shadow-2xl backdrop:bg-zinc-950/80"
    >
      <form onSubmit={onSubmit} noValidate aria-busy={submitting}>
        <div className="border-b border-zinc-800 px-5 py-4">
          <h2 id="add-download-title" className="text-[15px] font-semibold">
            Add a download
          </h2>
          <p id="add-download-description" className="mt-1 text-[12px] leading-relaxed text-zinc-400">
            Choose a URL and where it should be saved. The download starts when a queue slot opens.
          </p>
        </div>

        <div className="max-h-[min(65vh,34rem)] space-y-4 overflow-y-auto px-5 py-4">
          <div>
            <label htmlFor="download-url" className={LABEL}>Download URL</label>
            <input
              id="download-url"
              type="url"
              autoFocus
              value={draft.url}
              onChange={(event) => change("url", event.target.value)}
              aria-required="true"
              className={FIELD}
            />
          </div>
          <div>
            <label htmlFor="download-destination" className={LABEL}>
              Destination folder
            </label>
            <input
              id="download-destination"
              type="text"
              value={draft.destDir}
              onChange={(event) => change("destDir", event.target.value)}
              aria-describedby="download-destination-help"
              className={FIELD}
            />
            <p id="download-destination-help" className="mt-1 text-[11px] text-zinc-500">
              Leave blank to use your configured folder or your Downloads folder.
            </p>
          </div>
          <div>
            <label htmlFor="download-category" className={LABEL}>Category</label>
            <select
              id="download-category"
              value={draft.category}
              onChange={(event) => {
                const category = CATEGORIES.find((item) => item.id === event.target.value);
                if (category !== undefined) change("category", category.id);
              }}
              className={FIELD}
            >
              {CATEGORIES.map((item) => (
                <option key={item.id} value={item.id}>{item.label}</option>
              ))}
            </select>
          </div>

          <details className="rounded-md border border-zinc-800 px-3 py-2.5 text-[12px] text-zinc-400">
            <summary className="cursor-pointer font-medium text-zinc-300">Transfer options</summary>
            <div className="mt-4 grid grid-cols-2 gap-3">
              <div>
                <label htmlFor="download-priority" className={LABEL}>Priority</label>
                <input
                  id="download-priority"
                  type="number"
                  step="1"
                  value={draft.priority}
                  onChange={(event) => change("priority", event.target.value)}
                  className={FIELD}
                />
                <p className="mt-1 text-[11px] text-zinc-500">Higher values start first.</p>
              </div>
              <div>
                <label htmlFor="download-connections" className={LABEL}>Connections</label>
                <input
                  id="download-connections"
                  type="number"
                  min="1"
                  max="32"
                  step="1"
                  value={draft.connections}
                  onChange={(event) => change("connections", event.target.value)}
                  aria-describedby="download-connections-help"
                  className={FIELD}
                />
                <p id="download-connections-help" className="mt-1 text-[11px] text-zinc-500">
                  Leave blank for the engine to choose.
                </p>
              </div>
              <div className="col-span-2">
                <label htmlFor="download-speed" className={LABEL}>Speed ceiling (bytes/s)</label>
                <input
                  id="download-speed"
                  type="number"
                  min="1"
                  step="1"
                  value={draft.speedLimitBps}
                  onChange={(event) => change("speedLimitBps", event.target.value)}
                  className={FIELD}
                />
              </div>
              <div className="col-span-2">
                <label htmlFor="download-sha256" className={LABEL}>Expected SHA-256</label>
                <input
                  id="download-sha256"
                  type="text"
                  spellCheck={false}
                  value={draft.expectedSha256}
                  onChange={(event) => change("expectedSha256", event.target.value)}
                  aria-describedby="download-sha256-help"
                  className={`${FIELD} font-mono`}
                />
                <p id="download-sha256-help" className="mt-1 text-[11px] text-zinc-500">
                  An optional 64-character hexadecimal checksum.
                </p>
              </div>
            </div>
          </details>

          {error !== null && (
            <p role="alert" className="rounded-md border border-red-900/70 bg-red-950/40 px-3 py-2 text-[12px] text-red-200">
              {error}
            </p>
          )}
        </div>

        <div className="flex justify-end gap-2 border-t border-zinc-800 px-5 py-3">
          <Button
            className="bg-zinc-800 text-zinc-200 hover:bg-zinc-700"
            onClick={requestClose}
          >
            Close
          </Button>
          <Button type="submit">
            {submitting ? "Adding download…" : "Add download"}
          </Button>
        </div>
      </form>
    </dialog>
  );
}
