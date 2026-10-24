import { ClipboardPaste, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { inspectUrl, normalizeClipboardUrl } from "../../lib/fileMeta";
import { isDesktopShell, readClipboardText } from "../../lib/native";
import { useAddDownload } from "../../state/AddDownloadProvider";
import { Button } from "../ui/Button";
import { IconButton } from "../ui/IconButton";

/** A quiet prompt for a link the user copied elsewhere, never an automatic add. */
export function ClipboardPrompt() {
  const { openAdd, isOpen } = useAddDownload();
  const [url, setUrl] = useState<string | null>(null);
  const seen = useRef<string | null>(null);

  useEffect(() => {
    if (!isDesktopShell() || isOpen) return undefined;
    let active = true;
    let reading = false;

    const check = (): void => {
      if (!active || reading || !document.hasFocus()) return;
      reading = true;
      void readClipboardText()
        .then((text) => {
          if (!active) return;
          const link = normalizeClipboardUrl(text);
          if (link === null || link === seen.current) return;
          seen.current = link;
          setUrl(link);
        })
        .catch(() => {
          // Clipboard permission can be denied or unavailable; the toolbar
          // paste action remains available without interrupting the user.
        })
        .finally(() => {
          reading = false;
        });
    };

    const initial = window.setTimeout(check, 900);
    const onFocus = (): void => {
      window.setTimeout(check, 120);
    };
    window.addEventListener("focus", onFocus);
    return () => {
      active = false;
      window.clearTimeout(initial);
      window.removeEventListener("focus", onFocus);
    };
  }, [isOpen]);

  if (url === null || isOpen) return null;
  const info = inspectUrl(url);

  return (
    <aside
      aria-label="Link on the clipboard"
      className="fixed bottom-4 right-4 z-40 w-[min(23rem,calc(100vw-2rem))] overflow-hidden rounded-xl border border-line bg-panel/95 shadow-[0_22px_60px_-20px_rgba(0,0,0,0.9)] backdrop-blur rapids-dialog-enter"
    >
      <div className="flex items-start gap-2.5 px-3.5 py-3">
        <span className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-ink/10 text-ink">
          <ClipboardPaste size={15} strokeWidth={1.8} />
        </span>
        <div className="min-w-0 flex-1">
          <p className="text-[10px] font-semibold uppercase tracking-[0.09em] text-ink-soft">
            On the clipboard
          </p>
          <p
            className="mt-1 truncate text-[12.5px] font-medium text-ink"
            title={info.fileName ?? url}
          >
            {info.fileName ?? info.host ?? url}
          </p>
          <p className="mt-0.5 truncate text-[10.5px] text-muted" title={url}>
            {info.host} · {info.typeLabel}
          </p>
        </div>
        <IconButton
          label="Dismiss clipboard link"
          onClick={() => setUrl(null)}
          className="-mr-1 -mt-1 h-6 w-6"
        >
          <X size={13} strokeWidth={1.8} />
        </IconButton>
      </div>
      <div className="flex items-center justify-end gap-2 border-t border-line/70 bg-canvas/25 px-3.5 py-2">
        <Button variant="ghost" size="sm" onClick={() => setUrl(null)}>
          Dismiss
        </Button>
        <Button
          size="sm"
          onClick={() => {
            const link = url;
            setUrl(null);
            openAdd(link);
          }}
        >
          Review link
        </Button>
      </div>
    </aside>
  );
}
