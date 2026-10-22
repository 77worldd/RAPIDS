import { Globe2 } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent } from "react";
import {
  describeError,
  openSniffInspector,
  subscribeSniffCandidates,
} from "../../lib/ipc";
import {
  createSniffInbox,
  dismissSniffInbox,
  receiveSniffCandidate,
  selectSniffCandidate,
  toInspectorUrl,
} from "../../lib/sniff";
import { useStoreDispatch } from "../../state/StoreProvider";
import { AddDownloadDialog } from "../tasks/AddDownloadDialog";
import { MediaCandidatePopup } from "../tasks/MediaCandidatePopup";
import { Button } from "../ui/Button";

interface InspectUrlDialogProps {
  readonly onOpen: (url: string) => Promise<void>;
  readonly onClose: () => void;
}

function InspectUrlDialog({ onOpen, onClose }: InspectUrlDialogProps) {
  const dialog = useRef<HTMLDialogElement>(null);
  const inFlight = useRef(false);
  const [url, setUrl] = useState("");
  const [opening, setOpening] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const element = dialog.current;
    if (element !== null && !element.open) element.showModal();
    return () => {
      if (element?.open) element.close();
    };
  }, []);

  function requestClose(): void {
    if (inFlight.current) {
      setError("The inspector is opening. Wait for its result before closing.");
      return;
    }
    onClose();
  }

  async function submit(): Promise<void> {
    if (inFlight.current) {
      setError("The inspector is already opening.");
      return;
    }
    inFlight.current = true;
    setOpening(true);
    setError(null);
    try {
      await onOpen(url);
    } catch (cause) {
      inFlight.current = false;
      setOpening(false);
      setError(describeError(cause));
      return;
    }
    inFlight.current = false;
    onClose();
  }

  function onSubmit(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    void submit();
  }

  return (
    <dialog
      ref={dialog}
      aria-labelledby="inspect-page-title"
      aria-describedby="inspect-page-description"
      onCancel={(event) => {
        event.preventDefault();
        requestClose();
      }}
      className="w-[min(30rem,calc(100vw-2rem))] rounded-xl border border-zinc-700 bg-zinc-950 p-0 text-zinc-100 shadow-2xl backdrop:bg-zinc-950/80"
    >
      <form onSubmit={onSubmit} noValidate aria-busy={opening}>
        <div className="border-b border-zinc-800 px-5 py-4">
          <h2 id="inspect-page-title" className="text-[15px] font-semibold">
            Inspect a media page
          </h2>
          <p
            id="inspect-page-description"
            className="mt-1 text-[12px] leading-relaxed text-zinc-400"
          >
            Open a page in a separate inspector. Direct media found there is
            suggested in this window; nothing is added automatically.
          </p>
        </div>
        <div className="space-y-3 px-5 py-4">
          <div>
            <label
              htmlFor="inspect-page-url"
              className="mb-1.5 block text-[12px] font-medium text-zinc-300"
            >
              Page URL
            </label>
            <input
              id="inspect-page-url"
              type="url"
              autoFocus
              value={url}
              onChange={(event) => {
                setUrl(event.target.value);
                setError(null);
              }}
              aria-required="true"
              className="h-9 w-full rounded-md border border-zinc-700 bg-zinc-900 px-3 text-[13px] text-zinc-100 outline-none transition-colors duration-150 focus:border-emerald-500"
            />
          </div>
          {error !== null && (
            <p
              role="alert"
              className="rounded-md border border-red-900/70 bg-red-950/40 px-3 py-2 text-[12px] text-red-200"
            >
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
            {opening ? "Opening inspector…" : "Open page"}
          </Button>
        </div>
      </form>
    </dialog>
  );
}

export function InspectPageButton() {
  const dispatch = useStoreDispatch();
  const listener = useRef<ReturnType<typeof subscribeSniffCandidates> | null>(
    null,
  );
  const [promptOpen, setPromptOpen] = useState(false);
  const [selectedUrl, setSelectedUrl] = useState<string | null>(null);
  const [inbox, setInbox] = useState(createSniffInbox);

  useEffect(() => {
    let active = true;
    const pending = subscribeSniffCandidates(
      (candidate) => {
        if (active) {
          setInbox((current) => receiveSniffCandidate(current, candidate));
        }
      },
      (error) => {
        if (active)
          dispatch({ type: "notice/set", message: describeError(error) });
      },
    );
    listener.current = pending;
    void pending.catch((error: unknown) => {
      if (active)
        dispatch({ type: "notice/set", message: describeError(error) });
    });
    return () => {
      active = false;
      if (listener.current === pending) listener.current = null;
      void pending.then(
        (off) => off(),
        () => {},
      );
    };
  }, [dispatch]);

  return (
    <>
      <Button
        className="bg-zinc-800 text-zinc-200 hover:bg-zinc-700"
        onClick={() => setPromptOpen(true)}
      >
        <Globe2 size={14} strokeWidth={1.75} />
        Inspect page
      </Button>
      {promptOpen && (
        <InspectUrlDialog
          onClose={() => setPromptOpen(false)}
          onOpen={async (input) => {
            const url = toInspectorUrl(input);
            const ready = listener.current;
            if (ready === null) {
              throw new Error("The inspector listener is not ready.");
            }
            await ready;
            setInbox(createSniffInbox());
            await openSniffInspector(url);
          }}
        />
      )}
      {!promptOpen &&
        selectedUrl === null &&
        !inbox.dismissed &&
        inbox.pageUrl !== null &&
        inbox.candidates.length > 0 && (
          <MediaCandidatePopup
            pageUrl={inbox.pageUrl}
            candidates={inbox.candidates}
            onDismiss={() => setInbox((current) => dismissSniffInbox(current))}
            onSelect={(candidate) => {
              const choice = selectSniffCandidate(inbox, candidate);
              if (choice.url !== null) {
                setInbox(choice.inbox);
                setSelectedUrl(choice.url);
              }
            }}
          />
        )}
      {selectedUrl !== null && (
        <AddDownloadDialog
          initialUrl={selectedUrl}
          onClose={() => setSelectedUrl(null)}
        />
      )}
    </>
  );
}
