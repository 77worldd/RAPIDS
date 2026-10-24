import { Check, ChevronDown, Film, X } from "lucide-react";
import {
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import { cn } from "../../lib/cn";
import { formatBytes } from "../../lib/format";
import {
  buildMediaItems,
  type MediaItem,
  type MediaVariant,
} from "../../lib/media";
import { sniffPageMetadata } from "../../lib/sniff";
import type { SniffCandidate, SniffSource } from "../../types";
import { Button } from "../ui/Button";
import { IconButton } from "../ui/IconButton";

export interface MediaSelection {
  readonly url: string;
  readonly source: SniffSource;
}

interface MediaPickerProps {
  readonly pageUrl: string;
  readonly candidates: readonly SniffCandidate[];
  readonly onSelect: (selection: MediaSelection) => void;
  readonly onDismiss: () => void;
}

function hostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return "";
  }
}

function kindLabel(kind: MediaItem["kind"]): string {
  if (kind === "video") return "VIDEO";
  if (kind === "audio") return "AUDIO";
  return "FILE";
}

function formatDuration(seconds: number | null): string | null {
  if (seconds === null || !Number.isFinite(seconds) || seconds <= 0)
    return null;
  const total = Math.round(seconds);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const rest = total % 60;
  const pad = (value: number): string => String(value).padStart(2, "0");
  return hours > 0
    ? `${hours}:${pad(minutes)}:${pad(rest)}`
    : `${minutes}:${pad(rest)}`;
}

function detailParts(variant: MediaVariant): string[] {
  const parts: string[] = [];
  if (variant.codec !== null) parts.push(variant.codec);
  if (variant.format !== "Unknown") parts.push(variant.format);
  if (variant.size !== null) parts.push(formatBytes(variant.size));
  const duration = formatDuration(variant.duration);
  if (duration !== null) parts.push(duration);
  if (variant.kind === "video") {
    if (variant.hasAudio === false) parts.push("No audio");
    else if (variant.hasAudio === true) parts.push("Audio included");
  }
  return parts;
}

function variantFor(
  item: MediaItem,
  selected: Record<string, string>,
): MediaVariant {
  const chosen = item.variants.find(
    (variant) => variant.url === selected[item.id],
  );
  if (chosen !== undefined) return chosen;
  const preferred = item.variants.find(
    (variant) => variant.url === item.selectedVariantUrl,
  );
  const fallback = preferred ?? item.variants[0];
  if (fallback === undefined) {
    throw new Error("A grouped media item must contain at least one variant.");
  }
  return fallback;
}

function Thumbnail({
  item,
  className,
}: {
  readonly item: MediaItem;
  readonly className?: string;
}) {
  const [failedUrl, setFailedUrl] = useState<string | null>(null);
  const failed =
    item.thumbnailUrl !== null && failedUrl === item.thumbnailUrl;

  if (item.thumbnailUrl === null || failed) {
    return (
      <div
        className={cn(
          "flex items-center justify-center bg-elevated text-faint",
          className,
        )}
      >
        <Film size={18} strokeWidth={1.7} aria-hidden="true" />
      </div>
    );
  }

  return (
    <img
      src={item.thumbnailUrl}
      alt=""
      referrerPolicy="no-referrer"
      loading="lazy"
      decoding="async"
      onError={() => setFailedUrl(item.thumbnailUrl)}
      className={cn("object-cover", className)}
    />
  );
}

export function MediaPicker({
  pageUrl,
  candidates,
  onSelect,
  onDismiss,
}: MediaPickerProps) {
  const titleId = useId();
  const items = useMemo(() => {
    const page = sniffPageMetadata(candidates);
    return buildMediaItems({
      pageUrl,
      ...page,
      candidates: candidates.map((candidate) => ({
        url: candidate.url,
        source: candidate.source,
        title: candidate.meta.title,
        poster: candidate.meta.poster,
        height: candidate.meta.height,
        duration: candidate.meta.duration,
      })),
    });
  }, [candidates, pageUrl]);

  const [selectedByItem, setSelectedByItem] = useState<Record<string, string>>(
    {},
  );
  const [openMenuId, setOpenMenuId] = useState<string | null>(null);
  const [previewId, setPreviewId] = useState<string | null>(null);
  const triggerRefs = useRef(new Map<string, HTMLButtonElement>());
  const menuRefs = useRef(new Map<string, HTMLDivElement>());
  const pendingFocus = useRef<{ id: string; index: number } | null>(null);

  const previewItem =
    items.find((item) => item.id === previewId) ?? items[0] ?? null;
  const previewVariant =
    previewItem === null ? null : variantFor(previewItem, selectedByItem);
  const streamCount = items.reduce(
    (total, item) => total + item.variants.length,
    0,
  );
  const host = hostOf(pageUrl);

  useEffect(() => {
    if (openMenuId === null) return;
    const menu = menuRefs.current.get(openMenuId);
    const pending = pendingFocus.current;
    pendingFocus.current = null;
    if (menu === undefined) return;
    const options = menu.querySelectorAll<HTMLElement>('[role="option"]');
    const index =
      pending !== null && pending.id === openMenuId ? pending.index : 0;
    options[index]?.focus();
  }, [openMenuId]);

  useEffect(() => {
    if (openMenuId === null) return;
    function onDocumentClick(event: MouseEvent): void {
      if (!(event.target instanceof Element)) return;
      if (event.target.closest("[data-media-control]") !== null) return;
      setOpenMenuId(null);
    }
    document.addEventListener("click", onDocumentClick);
    return () => document.removeEventListener("click", onDocumentClick);
  }, [openMenuId]);

  function focusTrigger(id: string): void {
    window.requestAnimationFrame(() => {
      triggerRefs.current.get(id)?.focus();
    });
  }

  function closeMenu(restoreFocus: boolean): void {
    const id = openMenuId;
    setOpenMenuId(null);
    if (restoreFocus && id !== null) focusTrigger(id);
  }

  function openMenu(item: MediaItem): void {
    const current = variantFor(item, selectedByItem);
    const index = item.variants.findIndex(
      (variant) => variant.url === current.url,
    );
    pendingFocus.current = { id: item.id, index: index < 0 ? 0 : index };
    setOpenMenuId(item.id);
    setPreviewId(item.id);
  }

  function choose(item: MediaItem, variant: MediaVariant): void {
    setSelectedByItem((current) => ({ ...current, [item.id]: variant.url }));
    setPreviewId(item.id);
    setOpenMenuId(null);
    focusTrigger(item.id);
  }

  function onMenuKeyDown(event: KeyboardEvent<HTMLDivElement>): void {
    const options = Array.from(
      event.currentTarget.querySelectorAll<HTMLElement>('[role="option"]'),
    );
    const active =
      document.activeElement instanceof HTMLElement
        ? options.indexOf(document.activeElement)
        : -1;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      const next =
        active < 0
          ? step > 0
            ? 0
            : options.length - 1
          : (active + step + options.length) % options.length;
      options[next]?.focus();
      return;
    }
    if (event.key === "Home" || event.key === "End") {
      event.preventDefault();
      options[event.key === "Home" ? 0 : options.length - 1]?.focus();
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      options[active]?.click();
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      closeMenu(true);
      return;
    }
    if (event.key === "Tab") {
      event.preventDefault();
      closeMenu(true);
    }
  }

  if (items.length === 0 || previewItem === null || previewVariant === null) {
    return (
      <aside
        aria-labelledby={titleId}
        className="fixed right-4 bottom-4 z-50 w-[min(30rem,calc(100vw-2rem))] overflow-hidden rounded-xl border border-line-strong bg-canvas text-ink shadow-2xl"
      >
        <div className="flex items-start gap-2 border-b border-line px-4 py-3">
          <div className="min-w-0 flex-1">
            <h2 id={titleId} className="text-[13px] font-semibold">
              Media found
            </h2>
            <p className="truncate text-[11px] text-muted">On {host}</p>
          </div>
          <IconButton label="Dismiss media suggestions" onClick={onDismiss}>
            <X size={15} strokeWidth={1.75} />
          </IconButton>
        </div>
        <p className="px-4 py-8 text-center text-[12px] text-muted">
          No direct media could be grouped from this page.
        </p>
      </aside>
    );
  }

  return (
    <aside
      aria-labelledby={titleId}
      className="fixed right-4 bottom-4 z-50 flex max-h-[min(78vh,42rem)] w-[min(30rem,calc(100vw-2rem))] flex-col overflow-hidden rounded-xl border border-line-strong bg-canvas text-ink shadow-2xl"
    >
      <div className="flex items-start gap-2 border-b border-line px-4 py-3">
        <div className="min-w-0 flex-1">
          <h2 id={titleId} className="text-[13px] font-semibold">
            Media found
          </h2>
          <p className="truncate text-[11px] text-muted">
            {items.length} {items.length === 1 ? "item" : "items"} ·{" "}
            {streamCount} {streamCount === 1 ? "stream" : "streams"} on {host}
          </p>
        </div>
        <IconButton label="Dismiss media suggestions" onClick={onDismiss}>
          <X size={15} strokeWidth={1.75} />
        </IconButton>
      </div>

      <div className="relative h-32 shrink-0 overflow-hidden border-b border-line bg-elevated">
        <Thumbnail item={previewItem} className="h-full w-full opacity-90" />
        <div className="absolute inset-0 bg-linear-to-t from-black/85 via-black/25 to-transparent" />
        <div className="absolute inset-x-0 bottom-0 p-3">
          <p className="truncate text-[14px] font-semibold text-white">
            {previewItem.title}
          </p>
          <p className="mt-0.5 truncate text-[11px] text-white/70">
            {previewItem.host} · {previewItem.variants.length}{" "}
            {previewItem.variants.length === 1 ? "quality" : "qualities"} ·{" "}
            {[previewVariant.label, ...detailParts(previewVariant)].join(" · ")}
          </p>
        </div>
      </div>

      <ul className="min-h-0 flex-1 space-y-2 overflow-y-auto p-3">
        {items.map((item, itemIndex) => {
          const variant = variantFor(item, selectedByItem);
          const expanded = openMenuId === item.id;
          const menuId = `${titleId}-menu-${itemIndex}`;
          const details = detailParts(variant);
          return (
            <li
              key={item.id}
              onPointerEnter={() => setPreviewId(item.id)}
              onFocusCapture={() => setPreviewId(item.id)}
              className={cn(
                "rounded-lg border bg-panel/50 p-2.5 transition-colors duration-150",
                previewItem.id === item.id
                  ? "border-line-strong bg-panel"
                  : "border-line",
              )}
            >
              <div className="flex gap-2.5">
                <div className="h-14 w-14 shrink-0 overflow-hidden rounded-md border border-line">
                  <Thumbnail item={item} className="h-full w-full" />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex items-start gap-1.5">
                    <h3
                      className="line-clamp-2 min-w-0 flex-1 text-[12px] font-semibold leading-snug text-ink"
                      title={`${item.title} · ${item.host} · ${item.variants.length} ${
                        item.variants.length === 1 ? "quality" : "qualities"
                      }`}
                    >
                      {item.title}
                    </h3>
                    <span className="mt-0.5 inline-flex shrink-0 items-center rounded border border-line-strong bg-ink/[0.05] px-1 py-px text-[8px] font-bold tracking-[0.07em] text-ink-soft">
                      {kindLabel(item.kind)}
                    </span>
                  </div>
                  <p
                    className="mt-1 truncate font-mono text-[10px] text-faint"
                    title={item.technicalName}
                  >
                    {item.technicalName}
                  </p>
                  <p className="mt-1 text-[10px] leading-relaxed text-muted">
                    {[variant.label, ...details].join(" · ")}
                  </p>
                </div>
              </div>

              <div className="mt-2 flex items-center gap-2">
                <button
                  ref={(node) => {
                    if (node === null) triggerRefs.current.delete(item.id);
                    else triggerRefs.current.set(item.id, node);
                  }}
                  type="button"
                  data-media-control="true"
                  aria-haspopup="listbox"
                  aria-expanded={expanded}
                  aria-controls={expanded ? menuId : undefined}
                  aria-label={`Quality for ${item.title}: ${variant.label}`}
                  onClick={() => {
                    if (expanded) closeMenu(true);
                    else openMenu(item);
                  }}
                  onKeyDown={(event) => {
                    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
                      event.preventDefault();
                      openMenu(item);
                    }
                  }}
                  className="flex h-7 min-w-0 flex-1 items-center justify-between gap-2 rounded-md border border-line-strong bg-panel px-2 text-[11px] font-medium text-ink transition-colors duration-150 hover:border-ink-soft hover:bg-elevated focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/45"
                >
                  <span className="truncate">{variant.label}</span>
                  <ChevronDown
                    size={13}
                    strokeWidth={1.9}
                    aria-hidden="true"
                    className={cn(
                      "shrink-0 text-muted transition-transform duration-150",
                      expanded && "rotate-180",
                    )}
                  />
                </button>
                <Button
                  size="sm"
                  aria-label={`Review ${variant.label} of ${item.title}`}
                  onClick={() =>
                    onSelect({ url: variant.url, source: variant.source })
                  }
                >
                  Review
                </Button>
              </div>

              {expanded && (
                <div
                  ref={(node) => {
                    if (node === null) menuRefs.current.delete(item.id);
                    else menuRefs.current.set(item.id, node);
                  }}
                  id={menuId}
                  role="listbox"
                  aria-label={`Quality options for ${item.title}`}
                  data-media-control="true"
                  onKeyDown={(event) => onMenuKeyDown(event)}
                  className="mt-1.5 max-h-44 overflow-y-auto rounded-md border border-line bg-elevated p-1"
                >
                  {item.variants.map((option) => {
                    const selected = option.url === variant.url;
                    const optionDetails = detailParts(option);
                    return (
                      <div
                        key={option.url}
                        role="option"
                        aria-selected={selected}
                        tabIndex={-1}
                        onClick={() => choose(item, option)}
                        className={cn(
                          "grid cursor-pointer grid-cols-[14px_minmax(0,1fr)_auto] items-center gap-1.5 rounded px-1.5 py-1.5 outline-none transition-colors duration-150 hover:bg-ink/[0.06] focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ink/60",
                          selected && "bg-ink/[0.08]",
                        )}
                      >
                        <span className="flex justify-center text-ink">
                          {selected && (
                            <Check
                              size={11}
                              strokeWidth={2.4}
                              aria-hidden="true"
                            />
                          )}
                        </span>
                        <span className="truncate text-[11px] font-semibold text-ink">
                          {option.label}
                        </span>
                        <span className="max-w-52 truncate text-right text-[9.5px] text-muted">
                          {optionDetails.join(" · ")}
                        </span>
                      </div>
                    );
                  })}
                </div>
              )}
            </li>
          );
        })}
      </ul>

      <p className="border-t border-line px-4 py-2.5 text-[11px] leading-relaxed text-muted">
        Reviewing opens the add form. Nothing downloads until you confirm.
      </p>
    </aside>
  );
}
