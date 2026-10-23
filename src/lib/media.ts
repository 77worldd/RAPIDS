import type { SniffSource } from "../types";

const MAX_URL_LENGTH = 4096;
const MAX_NAME_LENGTH = 255;
const MAX_VARIANTS = 8;
const MAX_VARIANT_HEIGHT = 8192;
const TITLE_LIMIT = 300;

export type MediaKind = "video" | "audio" | "file";
export type MediaThumbnailKind = "poster" | "og" | "none";
export type MediaSource = SniffSource;

export interface MediaVariant {
  readonly url: string;
  readonly kind: MediaKind;
  readonly height: number | null;
  readonly label: string;
  readonly format: string;
  readonly size: number | null;
  readonly hasAudio: boolean | null;
  readonly codec: string | null;
  readonly source: MediaSource;
}

export interface MediaItem {
  readonly id: string;
  readonly title: string;
  readonly technicalName: string;
  readonly pageUrl: string;
  readonly host: string;
  readonly kind: MediaKind;
  readonly thumbnailUrl: string | null;
  readonly thumbnailKind: MediaThumbnailKind;
  readonly variants: readonly MediaVariant[];
  readonly selectedVariantUrl: string;
}

export interface MediaCandidateInput {
  readonly url: string;
  readonly name?: string | null;
  readonly contentType?: string | null;
  readonly size?: number | null;
  readonly kind?: MediaKind | null;
  readonly source?: MediaSource | null;
  readonly title?: string | null;
  readonly height?: number | null;
  readonly hasAudio?: boolean | null;
}

export interface MediaPageInput {
  readonly pageUrl: string;
  readonly pageTitle?: string | null;
  readonly siteName?: string | null;
  readonly ogTitle?: string | null;
  readonly twitterTitle?: string | null;
  readonly ogImage?: string | null;
  readonly twitterImage?: string | null;
  readonly poster?: string | null;
  readonly candidates: readonly MediaCandidateInput[];
}

export interface MediaTitleInput {
  readonly specific?: string | null;
  readonly ogTitle?: string | null;
  readonly twitterTitle?: string | null;
  readonly pageTitle?: string | null;
  readonly siteName?: string | null;
  readonly fileName?: string | null;
  readonly url?: string | null;
}

const VIDEO_EXTENSIONS = new Set([
  "mp4",
  "m4v",
  "mkv",
  "mov",
  "webm",
  "avi",
  "wmv",
  "mpg",
  "mpeg",
  "ts",
  "m2ts",
  "flv",
  "ogv",
]);

const AUDIO_EXTENSIONS = new Set([
  "mp3",
  "m4a",
  "aac",
  "flac",
  "wav",
  "ogg",
  "opus",
  "wma",
  "aiff",
  "oga",
]);

const FORMAT_LABELS: Record<string, string> = {
  mp4: "MP4",
  m4v: "M4V",
  mkv: "MKV",
  mov: "MOV",
  webm: "WebM",
  avi: "AVI",
  wmv: "WMV",
  mpg: "MPG",
  mpeg: "MPEG",
  ts: "TS",
  m2ts: "M2TS",
  flv: "FLV",
  ogv: "OGV",
  mp3: "MP3",
  m4a: "M4A",
  aac: "AAC",
  flac: "FLAC",
  wav: "WAV",
  ogg: "OGG",
  opus: "OPUS",
  wma: "WMA",
  aiff: "AIFF",
  oga: "OGA",
  m3u8: "HLS",
  mpd: "DASH",
};

const HEIGHT_TOKENS = new Map<string, number>([
  ["8k", 4320],
  ["4k", 2160],
  ["2k", 1440],
  ["uhd", 2160],
  ["fhd", 1080],
  ["qhd", 1440],
  ["hd", 720],
  ["sd", 480],
]);

const KNOWN_HEIGHTS = new Set([
  144, 240, 360, 480, 540, 576, 720, 1080, 1440, 2160, 4320,
]);

const KNOWN_BITRATES = new Set([
  32, 48, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
]);

const STRIPPED_TOKENS = new Set([
  "144p",
  "240p",
  "360p",
  "480p",
  "540p",
  "576p",
  "720p",
  "1080p",
  "1440p",
  "2160p",
  "4320p",
  "4k",
  "8k",
  "2k",
  "uhd",
  "fhd",
  "qhd",
  "hd",
  "sd",
  "hdr",
  "sdr",
  "10bit",
  "8bit",
  "h264",
  "h265",
  "x264",
  "x265",
  "hevc",
  "avc",
  "avc1",
  "vp8",
  "vp9",
  "vp09",
  "av1",
  "av01",
  "mpeg2",
  "mpeg4",
  "divx",
  "xvid",
  "aac",
  "ac3",
  "eac3",
  "dts",
  "opus",
  "flac",
  "mp3",
  "pcm",
  "audio",
  "video",
  "audioonly",
  "videoonly",
  "noaudio",
  "withaudio",
  "muted",
  "combined",
  "muxed",
  "adaptive",
  "dash",
  "hls",
  "stream",
  "original",
]);

const GENERIC_TITLES = new Set([
  "video",
  "audio",
  "media",
  "video player",
  "html5 video",
  "untitled",
  "untitled video",
  "untitled media",
  "download",
]);

function cleanText(value: unknown, limit = TITLE_LIMIT): string {
  if (typeof value !== "string") return "";
  const cleaned = value
    .replace(/[\p{Cc}]+/gu, " ")
    .replace(/\s+/g, " ")
    .trim();
  return cleaned.length > limit ? cleaned.slice(0, limit).trim() : cleaned;
}

function sanitizeName(value: unknown): string | null {
  if (typeof value !== "string") return null;
  let cleaned = "";
  for (const character of value) {
    const code = character.codePointAt(0) ?? 0;
    if (
      character === "/" ||
      character === "\\" ||
      code < 0x20 ||
      code === 0x7f
    ) {
      continue;
    }
    cleaned += character;
  }
  cleaned = cleaned.replace(/\s+/g, " ").trim();
  if (cleaned === "" || [...cleaned].length > MAX_NAME_LENGTH) return null;
  return cleaned;
}

function parseHttpUrl(value: unknown): URL | null {
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
      url.hostname === "" ||
      url.username !== "" ||
      url.password !== ""
    ) {
      return null;
    }
    return url;
  } catch {
    return null;
  }
}

function canonicalUrl(value: unknown): string | null {
  const url = parseHttpUrl(value);
  if (url === null) return null;
  url.hash = "";
  return url.href;
}

function lastSegment(url: URL): string | null {
  const segments = url.pathname.split("/").filter(Boolean);
  const segment = segments[segments.length - 1];
  if (segment === undefined || segment === "") return null;
  try {
    return sanitizeName(decodeURIComponent(segment));
  } catch {
    return sanitizeName(segment);
  }
}

function stripExtension(value: string): string {
  const name = sanitizeName(value) ?? "";
  const dot = name.lastIndexOf(".");
  if (dot <= 0 || dot === name.length - 1) return name;
  return name.slice(0, dot);
}

function extensionOf(value: string): string {
  const name = sanitizeName(value) ?? "";
  const dot = name.lastIndexOf(".");
  if (dot <= 0 || dot === name.length - 1) return "";
  const extension = name.slice(dot + 1).toLowerCase();
  return /^[a-z0-9]{1,8}$/.test(extension) ? extension : "";
}

function formatOf(name: string): string {
  const extension = extensionOf(name);
  if (extension === "") return "Unknown";
  return FORMAT_LABELS[extension] ?? extension.toUpperCase();
}

function kindFromContentType(contentType: unknown): MediaKind | null {
  if (typeof contentType !== "string") return null;
  const type = contentType.toLowerCase();
  if (type.startsWith("video/")) return "video";
  if (type.startsWith("audio/")) return "audio";
  return null;
}

function kindFromName(name: string): MediaKind {
  const extension = extensionOf(name);
  if (VIDEO_EXTENSIONS.has(extension)) return "video";
  if (AUDIO_EXTENSIONS.has(extension)) return "audio";
  return "file";
}

function validKind(value: unknown): value is MediaKind {
  return value === "video" || value === "audio" || value === "file";
}

function validSource(value: unknown): value is MediaSource {
  return (
    value === "video" ||
    value === "videoSource" ||
    value === "link" ||
    value === "navigation"
  );
}

function parseHeight(name: string): number | null {
  const lower = name.toLowerCase();
  const dimensions =
    /(?:^|[^0-9])(\d{3,4})\s*[x×]\s*(\d{3,4})(?:[^0-9]|$)/.exec(lower);
  if (dimensions !== null) {
    const height = Number(dimensions[2]);
    if (Number.isSafeInteger(height) && height >= 144) return height;
  }
  const suffixed = /(?:^|[^0-9])(\d{3,4})p(?:[^a-z0-9]|$)/.exec(lower);
  if (suffixed !== null) {
    const height = Number(suffixed[1]);
    if (KNOWN_HEIGHTS.has(height)) return height;
    if (Number.isSafeInteger(height) && height >= 144) return height;
  }
  const tokens = lower.split(/[^a-z0-9]+/).filter(Boolean);
  for (const token of tokens) {
    const known = HEIGHT_TOKENS.get(token);
    if (known !== undefined) return known;
    if (/^\d{3,4}$/.test(token)) {
      const height = Number(token);
      if (KNOWN_HEIGHTS.has(height)) return height;
    }
  }
  return null;
}

function explicitHeight(value: unknown): number | null {
  if (
    typeof value !== "number" ||
    !Number.isSafeInteger(value) ||
    value < 144 ||
    value > MAX_VARIANT_HEIGHT
  ) {
    return null;
  }
  return value;
}

function parseBitrate(name: string): number | null {
  const unit =
    /(?:^|[^a-z0-9])(\d{2,4})(?:[._](\d+))?\s*(kbps|kbit|k|mbps|mbit|m|bps)(?=$|[^a-z0-9])/i.exec(
      name,
    );
  if (unit !== null) {
    const whole = Number(unit[1]);
    const fraction = unit[2] === undefined ? 0 : Number(`0.${unit[2]}`);
    const unitText = unit[3] ?? "";
    const scale = unitText.toLowerCase().startsWith("m") ? 1000 : 1;
    const value = Math.round((whole + fraction) * scale);
    if (Number.isSafeInteger(value) && value > 0) return value;
  }
  const tokens = name
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter(Boolean);
  for (const token of tokens) {
    if (!/^\d{2,4}$/.test(token)) continue;
    const value = Number(token);
    if (KNOWN_BITRATES.has(value)) return value;
  }
  return null;
}

function parseCodec(name: string): string | null {
  const lower = name.toLowerCase();
  if (/(?:^|[^a-z0-9])h\.?264(?:[^0-9]|$)|x264/.test(lower)) return "H.264";
  if (/(?:^|[^a-z0-9])h\.?265(?:[^0-9]|$)|x265|hevc/.test(lower)) return "HEVC";
  const tokens = lower.split(/[^a-z0-9]+/).filter(Boolean);
  for (const token of tokens) {
    if (token === "vp9" || token === "vp09") return "VP9";
    if (token === "vp8") return "VP8";
    if (token === "av1" || token === "av01") return "AV1";
    if (token === "aac") return "AAC";
    if (token === "ac3") return "AC-3";
    if (token === "eac3") return "E-AC-3";
    if (token === "dts") return "DTS";
    if (token === "opus") return "Opus";
    if (token === "flac") return "FLAC";
    if (token === "mp3") return "MP3";
  }
  return null;
}

function parseHasAudio(name: string, kind: MediaKind): boolean | null {
  if (kind === "audio") return true;
  const lower = name.toLowerCase().replace(/[._-]+/g, " ");
  if (/\b(video only|no audio|muted)\b/.test(lower)) return false;
  if (/\b(audio|with audio|aac|ac3|eac3|dts|opus)\b/.test(lower)) {
    return true;
  }
  return null;
}

/** Strip quality, codec, stream, and bitrate tokens so variants of one media
 * file share a group key. Episode and title tokens are preserved. */
export function normalizeMediaStem(value: string): string {
  const name = stripExtension(sanitizeName(value) ?? "");
  const lower = name
    .toLowerCase()
    .replace(/\b(?:video|audio)[ ._-]?only\b/g, " ")
    .replace(/\bno[ ._-]?audio\b/g, " ")
    .replace(/\bwith[ ._-]?audio\b/g, " ")
    .replace(
      /(?:^|[^a-z0-9])\d{2,4}(?:[._]\d+)?\s*(?:kbps|kbits?|k|mbps|mbits?|m|bps)(?=$|[^a-z0-9])/g,
      " ",
    );
  const rawTokens = lower.split(/[^a-z0-9]+/).filter(Boolean);
  const kept = rawTokens.filter(
    (token) =>
      !STRIPPED_TOKENS.has(token) &&
      !/^\d{2,3}fps$/.test(token) &&
      !KNOWN_HEIGHTS.has(Number(token)) &&
      !KNOWN_BITRATES.has(Number(token)),
  );
  return (kept.length > 0 ? kept : rawTokens).join(" ").trim();
}

function variantLabel(
  kind: MediaKind,
  height: number | null,
  bitrate: number | null,
): string {
  if (kind === "audio") {
    return bitrate === null ? "Audio" : `Audio ${bitrate} kbps`;
  }
  if (height !== null) return `${height}p`;
  return "Original";
}

function compareVariants(left: MediaVariant, right: MediaVariant): number {
  const leftHeight = left.height ?? -1;
  const rightHeight = right.height ?? -1;
  if (leftHeight !== rightHeight) return rightHeight - leftHeight;
  const leftBitrate = parseBitrate(left.label) ?? 0;
  const rightBitrate = parseBitrate(right.label) ?? 0;
  if (leftBitrate !== rightBitrate) return rightBitrate - leftBitrate;
  if (left.label !== right.label) return left.label.localeCompare(right.label);
  return left.url.localeCompare(right.url);
}

/** Highest variant at or below 1080p; when everything is larger, the smallest
 * available option; otherwise the first sorted variant. */
export function selectDefaultVariant(
  variants: readonly MediaVariant[],
): MediaVariant | null {
  if (variants.length === 0) return null;
  const sorted = [...variants].sort(compareVariants);
  const known = sorted.filter((variant) => variant.height !== null);
  const below = known.filter((variant) => (variant.height ?? 0) <= 1080);
  if (below.length > 0) return below[0] ?? null;
  if (known.length > 0) return known[known.length - 1] ?? null;
  const visual = sorted.find((variant) => variant.kind !== "audio");
  return visual ?? sorted[0] ?? null;
}

function cleanPageTitle(value: unknown, siteName: string | null): string {
  let title = cleanText(value);
  if (title === "") return "";
  title = title.replace(/\s+(?:\||·|•)\s+.*$/, "").trim();
  const hyphen = /^(.*\S)\s+[—–-]\s+(.+)$/.exec(title);
  if (hyphen !== null) {
    const head = hyphen[1]?.trim() ?? "";
    const tail = hyphen[2]?.trim() ?? "";
    const tailLower = tail.toLowerCase();
    const siteLower = (siteName ?? "").toLowerCase();
    const looksLikeSite =
      (tailLower !== "" &&
        siteLower !== "" &&
        (tailLower === siteLower || tailLower.includes(siteLower))) ||
      (!/\s/.test(tail) && tail.length <= 24);
    if (head !== "" && looksLikeSite) title = head;
  }
  return cleanText(title);
}

function humanizeName(value: string | null): string {
  if (value === null) return "";
  return stripExtension(value)
    .replace(/[._-]+/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

/** Resolve a title without inventing metadata: specific label, Open Graph,
 * page title, filename, then the cleaned URL stem. */
export function resolveMediaTitle(input: MediaTitleInput): string {
  const specific = cleanText(input.specific);
  if (specific !== "" && !GENERIC_TITLES.has(specific.toLowerCase())) {
    return specific;
  }
  const og = cleanText(input.ogTitle);
  if (og !== "" && !GENERIC_TITLES.has(og.toLowerCase())) return og;
  const twitter = cleanText(input.twitterTitle);
  if (twitter !== "" && !GENERIC_TITLES.has(twitter.toLowerCase())) {
    return twitter;
  }
  const siteName = cleanText(input.siteName);
  const page = cleanPageTitle(
    input.pageTitle,
    siteName === "" ? null : siteName,
  );
  if (
    page !== "" &&
    !GENERIC_TITLES.has(page.toLowerCase()) &&
    page.toLowerCase() !== siteName.toLowerCase()
  ) {
    return page;
  }
  const file = humanizeName(input.fileName ?? null);
  if (file !== "" && !GENERIC_TITLES.has(file.toLowerCase())) return file;
  const parsedUrl = parseHttpUrl(input.url);
  const urlName = parsedUrl === null ? null : lastSegment(parsedUrl);
  const stem = normalizeMediaStem(urlName ?? "");
  if (stem !== "") return stem.charAt(0).toUpperCase() + stem.slice(1);
  return "Untitled media";
}

function mediaId(pageUrl: string, title: string, groupKey: string): string {
  const value = `${pageUrl}\n${title.trim().toLowerCase()}\n${groupKey}`;
  let hash = 0x811c9dc5;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return `media_${hash.toString(16).padStart(8, "0")}`;
}

function thumbnailFor(input: MediaPageInput): {
  readonly url: string | null;
  readonly kind: MediaThumbnailKind;
} {
  const poster = canonicalUrl(input.poster);
  if (poster !== null) return { url: poster, kind: "poster" };
  const openGraph = canonicalUrl(input.ogImage);
  if (openGraph !== null) return { url: openGraph, kind: "og" };
  const twitter = canonicalUrl(input.twitterImage);
  if (twitter !== null) return { url: twitter, kind: "og" };
  return { url: null, kind: "none" };
}

interface Prepared {
  readonly url: string;
  readonly name: string;
  readonly kind: MediaKind;
  readonly height: number | null;
  readonly label: string;
  readonly format: string;
  readonly size: number | null;
  readonly hasAudio: boolean | null;
  readonly codec: string | null;
  readonly source: MediaSource;
  readonly title: string;
  readonly groupKey: string;
}

function prepare(candidate: MediaCandidateInput): Prepared | null {
  const url = canonicalUrl(candidate.url);
  if (url === null) return null;
  const parsed = new URL(url);
  const name =
    sanitizeName(candidate.name) ?? lastSegment(parsed) ?? parsed.hostname;
  if (name === "") return null;
  const extension = extensionOf(name);
  if (extension === "m3u8" || extension === "mpd") return null;
  const kind = validKind(candidate.kind)
    ? candidate.kind
    : (kindFromContentType(candidate.contentType) ?? kindFromName(name));
  const height = explicitHeight(candidate.height) ?? parseHeight(name);
  const bitrate = kind === "audio" ? parseBitrate(name) : null;
  const size =
    typeof candidate.size === "number" &&
    Number.isSafeInteger(candidate.size) &&
    candidate.size > 0
      ? candidate.size
      : null;
  const hasAudio =
    typeof candidate.hasAudio === "boolean"
      ? candidate.hasAudio
      : parseHasAudio(name, kind);
  const family = kind === "file" ? "file" : "media";
  const stem = normalizeMediaStem(name);
  return {
    url,
    name,
    kind,
    height,
    label: variantLabel(kind, height, bitrate),
    format: formatOf(name),
    size,
    hasAudio,
    codec: parseCodec(name),
    source: validSource(candidate.source) ? candidate.source : "link",
    title: cleanText(candidate.title),
    groupKey: `${family}\u0000${stem}`,
  };
}

function mergePrepared(left: Prepared, right: Prepared): Prepared {
  return {
    ...left,
    name: left.name,
    kind:
      left.kind === "file" && right.kind !== "file" ? right.kind : left.kind,
    height: left.height ?? right.height,
    size: left.size ?? right.size,
    hasAudio: left.hasAudio ?? right.hasAudio,
    codec: left.codec ?? right.codec,
    source: left.source === "link" ? right.source : left.source,
    title: left.title !== "" ? left.title : right.title,
  };
}

function toVariant(prepared: Prepared): MediaVariant {
  return {
    url: prepared.url,
    kind: prepared.kind,
    height: prepared.height,
    label: prepared.label,
    format: prepared.format,
    size: prepared.size,
    hasAudio: prepared.hasAudio,
    codec: prepared.codec,
    source: prepared.source,
  };
}

/** Group raw candidates into media items. Candidates only merge when their
 * page, family, and quality-stripped stem match. */
export function buildMediaItems(input: MediaPageInput): readonly MediaItem[] {
  const pageUrl = canonicalUrl(input.pageUrl);
  if (pageUrl === null) return [];
  const page = new URL(pageUrl);
  const thumbnail = thumbnailFor(input);
  const unique = new Map<string, Prepared>();
  for (const candidate of input.candidates) {
    const prepared = prepare(candidate);
    if (prepared === null) continue;
    const existing = unique.get(prepared.url);
    if (existing === undefined) {
      unique.set(prepared.url, prepared);
    } else {
      unique.set(prepared.url, mergePrepared(existing, prepared));
    }
  }

  const groups = new Map<string, Prepared[]>();
  for (const prepared of unique.values()) {
    const bucket = groups.get(prepared.groupKey);
    if (bucket === undefined) groups.set(prepared.groupKey, [prepared]);
    else bucket.push(prepared);
  }

  const items: MediaItem[] = [];
  for (const [groupKey, bucket] of groups) {
    const variants = bucket
      .map(toVariant)
      .sort(compareVariants)
      .slice(0, MAX_VARIANTS);
    const selected = selectDefaultVariant(variants);
    if (selected === null) continue;
    const chosen =
      bucket.find((entry) => entry.url === selected.url) ?? bucket[0];
    if (chosen === undefined) continue;
    const title = resolveMediaTitle({
      specific: bucket.find((entry) => entry.title !== "")?.title ?? null,
      ogTitle: input.ogTitle ?? null,
      twitterTitle: input.twitterTitle ?? null,
      pageTitle: input.pageTitle ?? null,
      siteName: input.siteName ?? null,
      fileName: chosen.name,
      url: chosen.url,
    });
    const kind = variants.some((variant) => variant.kind === "video")
      ? "video"
      : variants.some((variant) => variant.kind === "audio")
        ? "audio"
        : "file";
    items.push({
      id: mediaId(pageUrl, title, groupKey),
      title,
      technicalName: chosen.name,
      pageUrl,
      host: page.host,
      kind,
      thumbnailUrl: thumbnail.url,
      thumbnailKind: thumbnail.kind,
      variants,
      selectedVariantUrl: selected.url,
    });
  }
  return items;
}
