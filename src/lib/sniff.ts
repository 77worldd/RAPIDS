import type {
  SniffCandidate,
  SniffMetadata,
  SniffPageMetadata,
} from "../types";

const MAX_URL_LENGTH = 2048;
const MAX_TEXT_LENGTH = 300;
const MAX_SITE_LENGTH = 120;
const MAX_HEIGHT = 8192;
const MAX_DURATION_SECONDS = 604_800;
const MAX_CANDIDATES = 16;
const MEDIA_EXTENSIONS = [".mp4", ".webm", ".m4v", ".mov", ".mkv"];

function httpUrl(value: unknown, canonical: boolean): URL | null {
  if (
    typeof value !== "string" ||
    value.length === 0 ||
    value.length > MAX_URL_LENGTH ||
    value.trim() !== value
  ) {
    return null;
  }
  try {
    const url = new URL(value);
    if (
      (url.protocol !== "http:" && url.protocol !== "https:") ||
      !url.hostname ||
      url.username !== "" ||
      url.password !== "" ||
      url.href.length > MAX_URL_LENGTH ||
      (canonical && (url.href !== value || value.includes("#")))
    ) {
      return null;
    }
    return url;
  } catch {
    return null;
  }
}

function directMedia(url: URL): boolean {
  const path = url.pathname.toLowerCase();
  return MEDIA_EXTENSIONS.some((extension) => path.endsWith(extension));
}

/** Only the user's request opens the isolated browsing session. */
export function toInspectorUrl(value: string): string {
  const parsed = httpUrl(value, false);
  if (parsed === null) {
    throw new Error(
      "Enter a complete HTTP or HTTPS page URL without credentials.",
    );
  }
  return parsed.href;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function textValue(value: unknown, limit: number): string | null {
  if (typeof value !== "string") return null;
  const text = value.replace(/\s+/g, " ").trim();
  if (text === "") return null;
  return text.length > limit ? text.slice(0, limit).trim() : text;
}

function imageValue(value: unknown): string | null {
  const url = httpUrl(value, false);
  if (url === null) return null;
  url.hash = "";
  return url.href;
}

function emptyMeta(): SniffMetadata {
  return {
    title: null,
    poster: null,
    height: null,
    duration: null,
    pageTitle: null,
    siteName: null,
    ogTitle: null,
    twitterTitle: null,
    ogImage: null,
    twitterImage: null,
  };
}

/** Normalize metadata without trusting the native event: bad optional fields
 * become null, and oversized text is clipped to the same limits as Rust. */
function toSniffMetadata(value: unknown): SniffMetadata | null {
  if (!isRecord(value)) return null;
  const height = value.height;
  const duration = value.duration;
  return {
    title: textValue(value.title, MAX_TEXT_LENGTH),
    poster: imageValue(value.poster),
    height:
      typeof height === "number" &&
      Number.isSafeInteger(height) &&
      height >= 1 &&
      height <= MAX_HEIGHT
        ? height
        : null,
    duration:
      typeof duration === "number" &&
      Number.isFinite(duration) &&
      duration > 0 &&
      duration <= MAX_DURATION_SECONDS
        ? duration
        : null,
    pageTitle: textValue(value.pageTitle, MAX_TEXT_LENGTH),
    siteName: textValue(value.siteName, MAX_SITE_LENGTH),
    ogTitle: textValue(value.ogTitle, MAX_TEXT_LENGTH),
    twitterTitle: textValue(value.twitterTitle, MAX_TEXT_LENGTH),
    ogImage: imageValue(value.ogImage),
    twitterImage: imageValue(value.twitterImage),
  };
}

/** Parse the native event at the IPC boundary without retaining extra fields. */
export function toSniffCandidate(value: unknown): SniffCandidate | null {
  if (!isRecord(value)) return null;
  const fields = ["session", "pageUrl", "url", "source"];
  const hasMeta = Object.prototype.hasOwnProperty.call(value, "meta");
  if (
    !fields.every((field) =>
      Object.prototype.hasOwnProperty.call(value, field),
    ) ||
    Object.keys(value).length !== (hasMeta ? fields.length + 1 : fields.length)
  ) {
    return null;
  }

  const session = value.session;
  const source = value.source;
  if (
    typeof session !== "number" ||
    !Number.isSafeInteger(session) ||
    session < 1 ||
    (source !== "video" &&
      source !== "videoSource" &&
      source !== "link" &&
      source !== "navigation")
  ) {
    return null;
  }

  const page = httpUrl(value.pageUrl, true);
  const media = httpUrl(value.url, true);
  if (page === null || media === null) return null;
  const path = media.pathname.toLowerCase();
  if (
    path.endsWith(".m3u8") ||
    path.endsWith(".mpd") ||
    ((source === "link" || source === "navigation") && !directMedia(media)) ||
    (source === "navigation" && page.href !== media.href)
  ) {
    return null;
  }

  const meta = hasMeta ? toSniffMetadata(value.meta) : emptyMeta();
  if (meta === null) return null;

  return { session, pageUrl: page.href, url: media.href, source, meta };
}

/** Prefer the newest non-null value; metadata arrives with each candidate but
 * a later scan can fill gaps from a video that has finished loading. */
export function mergeSniffMetadata(
  current: SniffMetadata,
  incoming: SniffMetadata,
): SniffMetadata {
  const next: SniffMetadata = {
    title: incoming.title ?? current.title,
    poster: incoming.poster ?? current.poster,
    height: incoming.height ?? current.height,
    duration: incoming.duration ?? current.duration,
    pageTitle: incoming.pageTitle ?? current.pageTitle,
    siteName: incoming.siteName ?? current.siteName,
    ogTitle: incoming.ogTitle ?? current.ogTitle,
    twitterTitle: incoming.twitterTitle ?? current.twitterTitle,
    ogImage: incoming.ogImage ?? current.ogImage,
    twitterImage: incoming.twitterImage ?? current.twitterImage,
  };
  return (Object.keys(next) as (keyof SniffMetadata)[]).every(
    (key) => next[key] === current[key],
  )
    ? current
    : next;
}

/** Ephemeral candidate list for one inspector session and page. */
export interface SniffInbox {
  readonly session: number | null;
  readonly pageUrl: string | null;
  readonly candidates: readonly SniffCandidate[];
  readonly dismissed: boolean;
}

export function createSniffInbox(): SniffInbox {
  return { session: null, pageUrl: null, candidates: [], dismissed: false };
}

export function receiveSniffCandidate(
  current: SniffInbox,
  candidate: SniffCandidate,
): SniffInbox {
  if (current.session !== null && candidate.session < current.session)
    return current;
  if (
    current.session !== candidate.session ||
    current.pageUrl !== candidate.pageUrl
  ) {
    return {
      session: candidate.session,
      pageUrl: candidate.pageUrl,
      candidates: [candidate],
      dismissed: false,
    };
  }
  if (current.dismissed) return current;
  const existing = current.candidates.find(
    (item) => item.url === candidate.url,
  );
  if (existing !== undefined) {
    const meta = mergeSniffMetadata(existing.meta, candidate.meta);
    if (meta === existing.meta) return current;
    return {
      ...current,
      candidates: current.candidates.map((item) =>
        item === existing ? { ...item, meta } : item,
      ),
    };
  }
  if (current.candidates.length >= MAX_CANDIDATES) return current;
  return { ...current, candidates: [...current.candidates, candidate] };
}

/** Page fields are repeated on every event; the first non-null value wins. */
export function sniffPageMetadata(
  candidates: readonly SniffCandidate[],
): SniffPageMetadata {
  const page = {
    pageTitle: null as string | null,
    siteName: null as string | null,
    ogTitle: null as string | null,
    twitterTitle: null as string | null,
    ogImage: null as string | null,
    twitterImage: null as string | null,
  };
  for (const candidate of candidates) {
    page.pageTitle ??= candidate.meta.pageTitle;
    page.siteName ??= candidate.meta.siteName;
    page.ogTitle ??= candidate.meta.ogTitle;
    page.twitterTitle ??= candidate.meta.twitterTitle;
    page.ogImage ??= candidate.meta.ogImage;
    page.twitterImage ??= candidate.meta.twitterImage;
  }
  return page;
}

/** Keep this navigation muted; another page or session starts a fresh inbox. */
export function dismissSniffInbox(current: SniffInbox): SniffInbox {
  if (current.session === null || current.dismissed) return current;
  return { ...current, candidates: [], dismissed: true };
}

/** Selection prepares the existing add form; detection never starts a task. */
export function selectSniffCandidate(
  current: SniffInbox,
  candidate: SniffCandidate,
): { readonly inbox: SniffInbox; readonly url: string | null } {
  if (
    current.dismissed ||
    current.session !== candidate.session ||
    current.pageUrl !== candidate.pageUrl ||
    !current.candidates.some(
      (item) => item.url === candidate.url && item.source === candidate.source,
    )
  ) {
    return { inbox: current, url: null };
  }
  return { inbox: dismissSniffInbox(current), url: candidate.url };
}

/** The full signed URL stays in memory for selection; never show its query. */
export function sniffCandidateLabel(candidate: SniffCandidate): string {
  const url = new URL(candidate.url);
  return `${url.host}${url.pathname}`;
}
