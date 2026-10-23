import {
  ArrowDown,
  ArrowUp,
  ClipboardCopy,
  ExternalLink,
  FolderOpen,
  Pause,
  Play,
  RotateCcw,
  Trash2,
  X,
} from "lucide-react";
import {
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { cn } from "../../lib/cn";
import type { Task } from "../../types";
import type { TaskActions } from "../../state/useTaskActions";

interface TaskActionMenuProps {
  readonly task: Task;
  readonly actions: TaskActions;
}

interface MenuPosition {
  readonly left: number;
  readonly top: number;
}

const MENU_WIDTH = 214;
const ESTIMATED_HEIGHT = 264;

export function TaskActionMenu({ task, actions }: TaskActionMenuProps) {
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState<MenuPosition | null>(null);

  useEffect(() => {
    if (position === null) return undefined;
    const onPointerDown = (event: PointerEvent): void => {
      const target = event.target;
      if (!(target instanceof Node)) return;
      if (trigger.current?.contains(target) || menu.current?.contains(target)) {
        return;
      }
      setPosition(null);
    };
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === "Escape") setPosition(null);
    };
    const close = (): void => setPosition(null);
    document.addEventListener("pointerdown", onPointerDown, true);
    document.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("scroll", close, true);
    window.addEventListener("resize", close);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      document.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("resize", close);
    };
  }, [position]);

  function open(): void {
    if (position !== null) {
      setPosition(null);
      return;
    }
    const rect = trigger.current?.getBoundingClientRect();
    if (rect === undefined) return;
    const left = Math.min(
      Math.max(8, rect.right - MENU_WIDTH),
      window.innerWidth - MENU_WIDTH - 8,
    );
    const top =
      rect.bottom + ESTIMATED_HEIGHT > window.innerHeight - 8
        ? Math.max(8, rect.top - ESTIMATED_HEIGHT - 4)
        : rect.bottom + 4;
    setPosition({ left, top });
  }

  function run(action: () => void): void {
    setPosition(null);
    action();
  }

  const canOpen = task.status === "completed";
  const partial = task.downloaded > 0;

  return (
    <>
      <button
        ref={trigger}
        type="button"
        aria-label={`Actions for ${task.fileName}`}
        aria-haspopup="menu"
        aria-expanded={position !== null}
        onClick={open}
        className="inline-flex h-7 w-7 items-center justify-center rounded-md text-zinc-500 transition-colors hover:bg-white/[0.06] hover:text-zinc-200 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-emerald-500/65"
      >
        <svg
          width="15"
          height="15"
          viewBox="0 0 24 24"
          fill="currentColor"
          aria-hidden="true"
        >
          <circle cx="5" cy="12" r="1.7" />
          <circle cx="12" cy="12" r="1.7" />
          <circle cx="19" cy="12" r="1.7" />
        </svg>
      </button>
      {position !== null &&
        createPortal(
          <div
            ref={menu}
            role="menu"
            aria-label={`Actions for ${task.fileName}`}
            style={{ left: position.left, top: position.top }}
            className="fixed z-[120] w-[214px] overflow-hidden rounded-lg border border-line bg-elevated p-1 shadow-[0_18px_48px_-12px_rgba(0,0,0,0.9)] rapids-dialog-enter"
          >
            {canOpen && (
              <>
                <MenuItem
                  icon={<ExternalLink size={13} strokeWidth={1.8} />}
                  label="Open file"
                  onClick={() => run(() => actions.openFile(task))}
                />
                <MenuItem
                  icon={<FolderOpen size={13} strokeWidth={1.8} />}
                  label="Show in folder"
                  onClick={() => run(() => actions.revealFile(task))}
                />
                <MenuDivider />
              </>
            )}
            {!canOpen && partial && (
              <>
                <MenuItem
                  icon={<FolderOpen size={13} strokeWidth={1.8} />}
                  label="Show partial file"
                  onClick={() => run(() => actions.revealFile(task))}
                />
                <MenuDivider />
              </>
            )}
            <MenuItem
              icon={<ClipboardCopy size={13} strokeWidth={1.8} />}
              label="Copy source link"
              onClick={() => run(() => actions.copyUrl(task))}
            />
            {task.status === "downloading" && (
              <MenuItem
                icon={<Pause size={13} strokeWidth={1.8} />}
                label="Pause"
                onClick={() => run(() => actions.pause(task))}
              />
            )}
            {task.status === "paused" && (
              <MenuItem
                icon={<Play size={13} strokeWidth={1.8} />}
                label="Resume"
                onClick={() => run(() => actions.resume(task))}
              />
            )}
            {task.status === "error" && (
              <MenuItem
                icon={<RotateCcw size={13} strokeWidth={1.8} />}
                label="Retry"
                onClick={() => run(() => actions.retry(task))}
              />
            )}
            {(task.status === "queued" || task.status === "scheduled") && (
              <>
                <MenuDivider />
                <MenuItem
                  icon={<ArrowUp size={13} strokeWidth={1.8} />}
                  label="Raise priority"
                  onClick={() => run(() => actions.reorder(task, 1))}
                />
                <MenuItem
                  icon={<ArrowDown size={13} strokeWidth={1.8} />}
                  label="Lower priority"
                  onClick={() => run(() => actions.reorder(task, -1))}
                />
              </>
            )}
            {(task.status === "downloading" ||
              task.status === "queued" ||
              task.status === "scheduled") && (
              <>
                <MenuDivider />
                <MenuItem
                  icon={<X size={13} strokeWidth={1.8} />}
                  label="Cancel transfer"
                  danger
                  onClick={() => run(() => actions.cancel(task))}
                />
              </>
            )}
            {(task.status === "paused" || task.status === "error") && (
              <>
                <MenuDivider />
                <MenuItem
                  icon={<Trash2 size={13} strokeWidth={1.8} />}
                  label="Remove from list"
                  danger
                  onClick={() => run(() => actions.remove(task, false))}
                />
              </>
            )}
            {task.status === "completed" && (
              <>
                <MenuDivider />
                <MenuItem
                  icon={<Trash2 size={13} strokeWidth={1.8} />}
                  label="Remove from list"
                  onClick={() => run(() => actions.remove(task, false))}
                />
                <MenuItem
                  icon={<Trash2 size={13} strokeWidth={1.8} />}
                  label="Delete from disk"
                  danger
                  onClick={() => run(() => actions.remove(task, true))}
                />
              </>
            )}
          </div>,
          document.body,
        )}
    </>
  );
}

function MenuItem({
  icon,
  label,
  onClick,
  danger = false,
}: {
  readonly icon: ReactNode;
  readonly label: string;
  readonly onClick: () => void;
  readonly danger?: boolean;
}) {
  return (
    <button
      type="button"
      role="menuitem"
      onClick={onClick}
      className={cn(
        "flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-[11.5px] transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-emerald-500/60",
        danger
          ? "text-red-300 hover:bg-red-500/10 hover:text-red-200"
          : "text-zinc-300 hover:bg-white/[0.055] hover:text-zinc-100",
      )}
    >
      <span className={cn("shrink-0", danger ? "text-red-400" : "text-zinc-500")}>
        {icon}
      </span>
      {label}
    </button>
  );
}

function MenuDivider() {
  return <div className="my-1 h-px bg-line/80" />;
}
