import type { SniffCandidate } from "../types";

const MAX_URL_LENGTH = 2048;
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

/** Parse the native event at the IPC boundary without retaining extra fields. */
export function toSniffCandidate(value: unknown): SniffCandidate | null {
  if (!isRecord(value)) return null;
  const fields = ["session", "pageUrl", "url", "source"];
  if (
    Object.keys(value).length !== fields.length ||
    !fields.every((field) => Object.prototype.hasOwnProperty.call(value, field))
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

  return { session, pageUrl: page.href, url: media.href, source };
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
  if (
    current.dismissed ||
    current.candidates.length >= MAX_CANDIDATES ||
    current.candidates.some((item) => item.url === candidate.url)
  ) {
    return current;
  }
  return { ...current, candidates: [...current.candidates, candidate] };
}

/** Keep this navigation muted; another page or session starts a fresh inbox. */
export function dismissSniffInbox(current: SniffInbox): SniffInbox {
  if (current.session === null || current.dismissed) return current;
  return { ...current, candidates: [], dismissed: true };
}

/** The full signed URL stays in memory for selection; never show its query. */
export function sniffCandidateLabel(candidate: SniffCandidate): string {
  const url = new URL(candidate.url);
  return `${url.host}${url.pathname}`;
}
