import { ArrowDownToLine } from "lucide-react";
import { useRef, useState, type DragEvent } from "react";
import { normalizeClipboardUrl } from "../../lib/fileMeta";
import { useAddDownload } from "../../state/AddDownloadProvider";
import { useStoreDispatch } from "../../state/StoreProvider";
import { TasksView } from "../../views/TasksView";
import { Sidebar } from "../sidebar/Sidebar";
import { TopBar } from "../topbar/TopBar";
import { ClipboardPrompt } from "./ClipboardPrompt";

function hasLinkPayload(event: DragEvent<HTMLDivElement>): boolean {
  return Array.from(event.dataTransfer.types).includes("text/uri-list") ||
    Array.from(event.dataTransfer.types).includes("text/plain");
}

export function AppShell() {
  const { openAdd, isOpen } = useAddDownload();
  const dispatch = useStoreDispatch();
  const dragDepth = useRef(0);
  const [dragging, setDragging] = useState(false);

  function resetDrag(): void {
    dragDepth.current = 0;
    setDragging(false);
  }

  return (
    <div
      className="flex h-screen w-screen overflow-hidden bg-canvas"
      onDragEnter={(event) => {
        if (isOpen || !hasLinkPayload(event)) return;
        event.preventDefault();
        dragDepth.current += 1;
        setDragging(true);
      }}
      onDragOver={(event) => {
        if (isOpen || !hasLinkPayload(event)) return;
        event.preventDefault();
        event.dataTransfer.dropEffect = "copy";
        setDragging(true);
      }}
      onDragLeave={(event) => {
        if (isOpen) return;
        event.preventDefault();
        dragDepth.current = Math.max(0, dragDepth.current - 1);
        if (dragDepth.current === 0) setDragging(false);
      }}
      onDrop={(event) => {
        if (isOpen) return;
        event.preventDefault();
        resetDrag();
        const raw =
          event.dataTransfer.getData("text/uri-list") ||
          event.dataTransfer.getData("text/plain");
        const url = normalizeClipboardUrl(raw);
        if (url === null) {
          dispatch({
            type: "notice/set",
            message: "Drop an HTTP or HTTPS download link to add it.",
          });
          return;
        }
        openAdd(url);
      }}
    >
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        <TopBar />
        <main className="min-h-0 flex-1 overflow-hidden">
          <TasksView />
        </main>
      </div>

      <ClipboardPrompt />

      {dragging && !isOpen && (
        <div className="pointer-events-none fixed inset-3 z-50 flex items-center justify-center rounded-2xl border-2 border-dashed border-accent-bright/60 bg-accent/[0.07] backdrop-blur-sm">
          <div className="flex max-w-sm flex-col items-center gap-3 rounded-2xl border border-accent-bright/25 bg-panel/95 px-8 py-7 text-center shadow-2xl">
            <span className="flex h-12 w-12 items-center justify-center rounded-xl bg-accent/12 text-accent-bright">
              <ArrowDownToLine size={22} strokeWidth={1.8} />
            </span>
            <div>
              <p className="text-[14px] font-semibold text-zinc-100">
                Drop the link to add it
              </p>
              <p className="mt-1 text-[11.5px] leading-relaxed text-zinc-500">
                RAPIDS will check the file name, type, and destination before queueing the transfer.
              </p>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
