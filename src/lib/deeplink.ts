/** The wire shape emitted by the host's browser handoff receiver. */

export type DeepLinkKind = "video" | "audio" | "file";

export interface DeepLinkRequest {
  readonly url: string;
  readonly name: string | null;
  readonly kind: DeepLinkKind;
  readonly sourcePage: string | null;
  readonly size: number | null;
}

export type PendingDeepLink =
  | { readonly kind: "add"; readonly request: DeepLinkRequest }
  | { readonly kind: "error"; readonly reason: string };

const MAX_URL_LENGTH = 4096;
const MAX_NAME_LENGTH = 255;
const MAX_REASON_LENGTH = 500;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function httpUrl(value: unknown): string | null {
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
      url.hostname === ""
    ) {
      return null;
    }
    return url.href;
  } catch {
    return null;
  }
}

function optionalName(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const name = value.trim();
  if (name === "" || [...name].length > MAX_NAME_LENGTH) return null;
  if (
    [...name].some((character) => {
      const code = character.codePointAt(0) ?? 0;
      return (
        character === "/" || character === "\\" || code < 0x20 || code === 0x7f
      );
    })
  ) {
    return null;
  }
  return name;
}

function optionalKind(value: unknown): DeepLinkKind {
  return value === "video" || value === "audio" ? value : "file";
}

function optionalSize(value: unknown): number | null {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0
    ? value
    : null;
}

/**
 * Validate one browser handoff before the UI sees it. Optional hints are
 * discarded when malformed; the required download URL is rejected outright.
 */
export function parseDeepLinkRequest(value: unknown): DeepLinkRequest | null {
  if (!isRecord(value)) return null;
  const url = httpUrl(value.url);
  if (url === null) return null;
  return {
    url,
    name: optionalName(value.name),
    kind: optionalKind(value.kind),
    sourcePage: httpUrl(value.sourcePage),
    size: optionalSize(value.size),
  };
}

export function parsePendingDeepLink(value: unknown): PendingDeepLink | null {
  if (!isRecord(value)) return null;
  if (value.kind === "add") {
    const request = parseDeepLinkRequest(value.request);
    return request === null ? null : { kind: "add", request };
  }
  if (value.kind === "error" && typeof value.reason === "string") {
    const reason = value.reason.trim();
    if (reason === "") return null;
    return { kind: "error", reason: reason.slice(0, MAX_REASON_LENGTH) };
  }
  return null;
}
