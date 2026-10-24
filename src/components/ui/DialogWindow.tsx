import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { X } from "lucide-react";
import { cn } from "../../lib/cn";
import { IconButton } from "./IconButton";

type ResizeEdge = "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";

interface Rect {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

interface DialogWindowProps {
  readonly title: string;
  readonly description?: string;
  readonly icon?: ReactNode;
  readonly children: ReactNode;
  readonly onClose: () => void;
  readonly initialWidth?: number;
  readonly initialHeight?: number;
  readonly minWidth?: number;
  readonly minHeight?: number;
  readonly bodyClassName?: string;
  readonly headerActions?: ReactNode;
}

const VIEWPORT_MARGIN = 8;
const VIEWPORT_GAP = 16;
const DEFAULT_WIDTH = 880;
const DEFAULT_HEIGHT = 680;
const DEFAULT_MIN_WIDTH = 620;
const DEFAULT_MIN_HEIGHT = 500;

function viewport() {
  return {
    width: Math.max(320, window.innerWidth),
    height: Math.max(320, window.innerHeight),
  };
}

function initialRect(
  width: number,
  height: number,
  minWidth: number,
  minHeight: number,
): Rect {
  const screen = viewport();
  const availableWidth = Math.max(320, screen.width - VIEWPORT_GAP * 2);
  const availableHeight = Math.max(320, screen.height - VIEWPORT_GAP * 2);
  const nextWidth = Math.min(Math.max(width, Math.min(minWidth, availableWidth)), availableWidth);
  const nextHeight = Math.min(
    Math.max(height, Math.min(minHeight, availableHeight)),
    availableHeight,
  );
  return {
    width: nextWidth,
    height: nextHeight,
    x: Math.round((screen.width - nextWidth) / 2),
    y: Math.round((screen.height - nextHeight) / 2),
  };
}

function clamp(value: number, low: number, high: number): number {
  return Math.min(high, Math.max(low, value));
}

function constrainedRect(
  rect: Rect,
  minWidth: number,
  minHeight: number,
): Rect {
  const screen = viewport();
  const maximumWidth = Math.max(320, screen.width - VIEWPORT_MARGIN * 2);
  const maximumHeight = Math.max(320, screen.height - VIEWPORT_MARGIN * 2);
  const width = clamp(rect.width, Math.min(minWidth, maximumWidth), maximumWidth);
  const height = clamp(rect.height, Math.min(minHeight, maximumHeight), maximumHeight);
  return {
    width,
    height,
    x: clamp(rect.x, VIEWPORT_MARGIN, Math.max(VIEWPORT_MARGIN, screen.width - width - VIEWPORT_MARGIN)),
    y: clamp(rect.y, VIEWPORT_MARGIN, Math.max(VIEWPORT_MARGIN, screen.height - height - VIEWPORT_MARGIN)),
  };
}

function resizedRect(
  start: Rect,
  edge: ResizeEdge,
  dx: number,
  dy: number,
  minWidth: number,
  minHeight: number,
): Rect {
  const screen = viewport();
  const maxRight = screen.width - VIEWPORT_MARGIN;
  const maxBottom = screen.height - VIEWPORT_MARGIN;
  const minimumWidth = Math.min(minWidth, maxRight - VIEWPORT_MARGIN);
  const minimumHeight = Math.min(minHeight, maxBottom - VIEWPORT_MARGIN);
  let left = start.x;
  let top = start.y;
  let right = start.x + start.width;
  let bottom = start.y + start.height;

  if (edge.includes("w")) {
    left = clamp(start.x + dx, VIEWPORT_MARGIN, right - minimumWidth);
  }
  if (edge.includes("e")) {
    right = clamp(start.x + start.width + dx, left + minimumWidth, maxRight);
  }
  if (edge.includes("n")) {
    top = clamp(start.y + dy, VIEWPORT_MARGIN, bottom - minimumHeight);
  }
  if (edge.includes("s")) {
    bottom = clamp(start.y + start.height + dy, top + minimumHeight, maxBottom);
  }

  return {
    x: left,
    y: top,
    width: right - left,
    height: bottom - top,
  };
}

function isInteractive(target: EventTarget | null): boolean {
  return (
    target instanceof Element &&
    target.closest("button, a, input, textarea, select, label, [data-no-drag]") !== null
  );
}

export function DialogWindow({
  title,
  description,
  icon,
  children,
  onClose,
  initialWidth = DEFAULT_WIDTH,
  initialHeight = DEFAULT_HEIGHT,
  minWidth = DEFAULT_MIN_WIDTH,
  minHeight = DEFAULT_MIN_HEIGHT,
  bodyClassName,
  headerActions,
}: DialogWindowProps) {
  const titleId = useId();
  const descriptionId = useId();
  const panelRef = useRef<HTMLDivElement>(null);
  const closeRef = useRef(onClose);

  useEffect(() => {
    closeRef.current = onClose;
  }, [onClose]);

  const [rect, setRect] = useState<Rect>(() =>
    initialRect(initialWidth, initialHeight, minWidth, minHeight),
  );
  const [interacting, setInteracting] = useState(false);

  const onWindowResize = useCallback(() => {
    setRect((current) => constrainedRect(current, minWidth, minHeight));
  }, [minHeight, minWidth]);

  useEffect(() => {
    window.addEventListener("resize", onWindowResize);
    return () => window.removeEventListener("resize", onWindowResize);
  }, [onWindowResize]);

  useEffect(() => {
    const previous = document.activeElement;
    const panel = panelRef.current;
    if (panel === null) return undefined;

    const focusables = (): HTMLElement[] =>
      Array.from(
        panel.querySelectorAll<HTMLElement>(
          "a[href], button:not([disabled]):not([tabindex='-1']), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex='-1'])",
        ),
      ).filter((element) => element.offsetParent !== null);

    const focusTarget =
      panel.querySelector<HTMLElement>("[data-autofocus]") ?? focusables()[0];
    focusTarget?.focus();

    const handleKeyDown = (event: KeyboardEvent): void => {
      if (event.key === "Escape") {
        event.preventDefault();
        closeRef.current();
        return;
      }
      if (event.key !== "Tab") return;
      const candidates = focusables();
      if (candidates.length === 0) {
        event.preventDefault();
        return;
      }
      const first = candidates[0];
      const last = candidates[candidates.length - 1];
      if (first === undefined || last === undefined) return;
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };

    document.addEventListener("keydown", handleKeyDown, true);
    return () => {
      document.removeEventListener("keydown", handleKeyDown, true);
      if (previous instanceof HTMLElement) previous.focus();
    };
  }, []);

  function finishInteraction(): void {
    setInteracting(false);
    document.body.classList.remove("rapids-interacting");
  }

  function beginDrag(event: ReactPointerEvent<HTMLDivElement>): void {
    if (event.button !== 0 || isInteractive(event.target)) return;
    event.preventDefault();
    const start = rect;
    const originX = event.clientX;
    const originY = event.clientY;
    setInteracting(true);
    document.body.classList.add("rapids-interacting");

    const move = (moveEvent: PointerEvent): void => {
      moveEvent.preventDefault();
      setRect(
        constrainedRect(
          {
            ...start,
            x: start.x + moveEvent.clientX - originX,
            y: start.y + moveEvent.clientY - originY,
          },
          minWidth,
          minHeight,
        ),
      );
    };
    const stop = (): void => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
      finishInteraction();
    };
    window.addEventListener("pointermove", move, { passive: false });
    window.addEventListener("pointerup", stop);
    window.addEventListener("pointercancel", stop);
  }

  function beginResize(
    event: ReactPointerEvent<HTMLButtonElement>,
    edge: ResizeEdge,
  ): void {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    const start = rect;
    const originX = event.clientX;
    const originY = event.clientY;
    setInteracting(true);
    document.body.classList.add("rapids-interacting");

    const move = (moveEvent: PointerEvent): void => {
      moveEvent.preventDefault();
      setRect(
        resizedRect(
          start,
          edge,
          moveEvent.clientX - originX,
          moveEvent.clientY - originY,
          minWidth,
          minHeight,
        ),
      );
    };
    const stop = (): void => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
      finishInteraction();
    };
    window.addEventListener("pointermove", move, { passive: false });
    window.addEventListener("pointerup", stop);
    window.addEventListener("pointercancel", stop);
  }

  const style: CSSProperties = {
    left: rect.x,
    top: rect.y,
    width: rect.width,
    height: rect.height,
  };

  return createPortal(
    <div
      className={cn(
        "fixed inset-0 z-[100] bg-zinc-950/75 backdrop-blur-[2px] rapids-dialog-backdrop",
        interacting && "cursor-default",
      )}
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) {
          closeRef.current();
        }
      }}
    >
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        {...(description !== undefined ? { "aria-describedby": descriptionId } : {})}
        style={style}
        className={cn(
          "fixed flex flex-col overflow-hidden rounded-xl border border-line bg-panel text-zinc-100 shadow-[0_28px_90px_-18px_rgba(0,0,0,0.85)] rapids-dialog-enter",
          interacting && "select-none",
        )}
      >
        <div
          onPointerDown={beginDrag}
          className="flex min-h-14 shrink-0 cursor-grab items-center gap-3 border-b border-line/80 px-4 py-3 active:cursor-grabbing"
        >
          {icon !== undefined && (
            <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-accent/10 text-accent-bright">
              {icon}
            </div>
          )}
          <div className="min-w-0 flex-1">
            <h2 id={titleId} className="truncate text-[14px] font-semibold tracking-[-0.01em] text-high">
              {title}
            </h2>
            {description !== undefined && (
              <p id={descriptionId} className="mt-0.5 truncate text-[11px] text-muted">
                {description}
              </p>
            )}
          </div>
          {headerActions}
          <div data-no-drag>
            <IconButton label="Close window" onClick={onClose}>
              <X size={16} strokeWidth={1.8} />
            </IconButton>
          </div>
        </div>

        <div className={cn("flex min-h-0 flex-1 flex-col overflow-hidden", bodyClassName)}>
          {children}
        </div>

        <ResizeHandles onResize={beginResize} />
      </div>
    </div>,
    document.body,
  );
}

function ResizeHandles({
  onResize,
}: {
  readonly onResize: (
    event: ReactPointerEvent<HTMLButtonElement>,
    edge: ResizeEdge,
  ) => void;
}) {
  const edges: readonly {
    readonly edge: ResizeEdge;
    readonly className: string;
  }[] = [
    { edge: "n", className: "inset-x-3 top-0 h-1.5 cursor-ns-resize" },
    { edge: "s", className: "inset-x-3 bottom-0 h-1.5 cursor-ns-resize" },
    { edge: "e", className: "inset-y-3 right-0 w-1.5 cursor-ew-resize" },
    { edge: "w", className: "inset-y-3 left-0 w-1.5 cursor-ew-resize" },
    { edge: "ne", className: "right-0 top-0 h-3 w-3 cursor-nesw-resize" },
    { edge: "nw", className: "left-0 top-0 h-3 w-3 cursor-nwse-resize" },
    { edge: "se", className: "bottom-0 right-0 h-3 w-3 cursor-nwse-resize" },
    { edge: "sw", className: "bottom-0 left-0 h-3 w-3 cursor-nesw-resize" },
  ];
  return (
    <>
      {edges.map(({ edge, className }) => (
        <button
          key={edge}
          type="button"
          tabIndex={-1}
          aria-label={`Resize window ${edge}`}
          className={cn(
            "absolute z-20 rounded-sm border-0 bg-transparent p-0 focus:outline-none",
            className,
          )}
          onPointerDown={(event) => onResize(event, edge)}
        />
      ))}
      <div className="pointer-events-none absolute bottom-1.5 right-1.5 flex h-3 w-3 flex-col items-end justify-end gap-[2px] opacity-50">
        <span className="h-px w-2.5 bg-zinc-500" />
        <span className="h-px w-1.5 bg-zinc-500" />
      </div>
    </>
  );
}
