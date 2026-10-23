import type { TagId } from "../types";

export type FileKind =
  | "video"
  | "audio"
  | "image"
  | "document"
  | "archive"
  | "software"
  | "other";

const CATEGORY_BY_EXTENSION: Readonly<Record<string, TagId>> = {
  mp4: "movies",
  m4v: "movies",
  mkv: "movies",
  mov: "movies",
  webm: "movies",
  avi: "movies",
  wmv: "movies",
  mpg: "movies",
  mpeg: "movies",
  ts: "movies",
  m2ts: "movies",
  mp3: "music",
  m4a: "music",
  aac: "music",
  flac: "music",
  wav: "music",
  ogg: "music",
  opus: "music",
  wma: "music",
  aiff: "music",
  jpg: "images",
  jpeg: "images",
  png: "images",
  gif: "images",
  webp: "images",
  avif: "images",
  bmp: "images",
  tif: "images",
  tiff: "images",
  svg: "images",
  heic: "images",
  pdf: "documents",
  doc: "documents",
  docx: "documents",
  xls: "documents",
  xlsx: "documents",
  ppt: "documents",
  pptx: "documents",
  txt: "documents",
  md: "documents",
  rtf: "documents",
  csv: "documents",
  epub: "documents",
  mobi: "documents",
  odt: "documents",
  ods: "documents",
  zip: "archives",
  rar: "archives",
  "7z": "archives",
  tar: "archives",
  gz: "archives",
  bz2: "archives",
  xz: "archives",
  zst: "archives",
  iso: "archives",
  img: "archives",
  exe: "apps",
  msi: "apps",
  msix: "apps",
  appx: "apps",
  apk: "apps",
  dmg: "apps",
  pkg: "apps",
  deb: "apps",
  rpm: "apps",
  jar: "apps",
};

const KIND_BY_EXTENSION: Readonly<Record<string, FileKind>> = {
  mp4: "video",
  m4v: "video",
  mkv: "video",
  mov: "video",
  webm: "video",
  avi: "video",
  wmv: "video",
  mpg: "video",
  mpeg: "video",
  ts: "video",
  m2ts: "video",
  m3u8: "video",
  mpd: "video",
  mp3: "audio",
  m4a: "audio",
  aac: "audio",
  flac: "audio",
  wav: "audio",
  ogg: "audio",
  opus: "audio",
  wma: "audio",
  aiff: "audio",
  jpg: "image",
  jpeg: "image",
  png: "image",
  gif: "image",
  webp: "image",
  avif: "image",
  bmp: "image",
  tif: "image",
  tiff: "image",
  svg: "image",
  heic: "image",
  pdf: "document",
  doc: "document",
  docx: "document",
  xls: "document",
  xlsx: "document",
  ppt: "document",
  pptx: "document",
  txt: "document",
  md: "document",
  rtf: "document",
  csv: "document",
  epub: "document",
  mobi: "document",
  odt: "document",
  ods: "document",
  zip: "archive",
  rar: "archive",
  "7z": "archive",
  tar: "archive",
  gz: "archive",
  bz2: "archive",
  xz: "archive",
  zst: "archive",
  iso: "archive",
  img: "archive",
  exe: "software",
  msi: "software",
  msix: "software",
  appx: "software",
  apk: "software",
  dmg: "software",
  pkg: "software",
  deb: "software",
  rpm: "software",
  jar: "software",
};

const TYPE_LABEL_BY_EXTENSION: Readonly<Record<string, string>> = {
  mp4: "MP4 video",
  m4v: "M4V video",
  mkv: "Matroska video",
  mov: "QuickTime video",
  webm: "WebM video",
  avi: "AVI video",
  m3u8: "HLS playlist",
  mpd: "MPEG-DASH manifest",
  mp3: "MP3 audio",
  m4a: "M4A audio",
  aac: "AAC audio",
  flac: "FLAC audio",
  wav: "WAV audio",
  ogg: "Ogg audio",
  opus: "Opus audio",
  jpg: "JPEG image",
  jpeg: "JPEG image",
  png: "PNG image",
  gif: "GIF image",
  webp: "WebP image",
  avif: "AVIF image",
  svg: "SVG image",
  pdf: "PDF document",
  docx: "Word document",
  xlsx: "Excel workbook",
  pptx: "PowerPoint deck",
  csv: "CSV data",
  epub: "EPUB book",
  zip: "ZIP archive",
  rar: "RAR archive",
  "7z": "7z archive",
  tar: "Tar archive",
  gz: "Gzip archive",
  iso: "Disc image",
  exe: "Windows installer",
  msi: "Windows installer",
  msix: "Windows package",
  apk: "Android package",
  dmg: "macOS disk image",
  pkg: "macOS installer",
  deb: "Debian package",
  rpm: "RPM package",
  jar: "Java archive",
};

const STREAMING_EXTENSIONS = new Set(["m3u8", "mpd"]);
const PAGE_EXTENSIONS = new Set(["html", "htm", "php", "asp", "aspx", "jsp"]);

export interface UrlFileInfo {
  readonly raw: string;
  readonly valid: boolean;
  readonly error: string | null;
  readonly url: URL | null;
  readonly fileName: string | null;
  readonly extension: string | null;
  readonly typeLabel: string;
  readonly kind: FileKind;
  readonly category: TagId;
  readonly isStreaming: boolean;
  readonly isPageLike: boolean;
  readonly host: string;
}

function invalid(raw: string, error: string): UrlFileInfo {
  return {
    raw,
    valid: false,
    error,
    url: null,
    fileName: null,
    extension: null,
    typeLabel: "Unknown file type",
    kind: "other",
    category: "other",
    isStreaming: false,
    isPageLike: false,
    host: "",
  };
}

function decodedSegment(segment: string): string | null {
  try {
    const decoded = decodeURIComponent(segment).trim();
    return decoded === "" || decoded === "." || decoded === ".."
      ? null
      : decoded;
  } catch {
    return null;
  }
}

/** Describes what the URL itself reveals. The server can still change the name. */
export function inspectUrl(raw: string): UrlFileInfo {
  const value = raw.trim();
  if (value === "") return invalid(raw, "");

  let parsed: URL;
  try {
    parsed = new URL(value);
  } catch {
    return invalid(raw, "Enter a complete HTTP or HTTPS link.");
  }
  if (
    (parsed.protocol !== "http:" && parsed.protocol !== "https:") ||
    parsed.hostname === ""
  ) {
    return invalid(raw, "Only HTTP and HTTPS links can be downloaded.");
  }
  if (parsed.username !== "" || parsed.password !== "") {
    return invalid(raw, "A download link cannot contain a username or password.");
  }

  const path = parsed.pathname;
  const segments = path.endsWith("/")
    ? []
    : path.split("/").filter((segment) => segment !== "");
  const finalSegment = segments[segments.length - 1] ?? "";
  const decoded = decodedSegment(finalSegment);
  const dot = decoded?.lastIndexOf(".") ?? -1;
  const extension =
    decoded !== null && dot > 0 && dot < decoded.length - 1
      ? decoded.slice(dot + 1).toLowerCase()
      : null;
  const category = extension === null ? "other" : (CATEGORY_BY_EXTENSION[extension] ?? "other");
  const kind = extension === null ? "other" : (KIND_BY_EXTENSION[extension] ?? "other");
  const isStreaming = extension !== null && STREAMING_EXTENSIONS.has(extension);
  const isPageLike = extension !== null && PAGE_EXTENSIONS.has(extension);

  return {
    raw,
    valid: true,
    error: null,
    url: parsed,
    fileName: decoded,
    extension,
    typeLabel:
      extension === null
        ? "Unknown file type"
        : (TYPE_LABEL_BY_EXTENSION[extension] ?? `${extension.toUpperCase()} file`),
    kind,
    category,
    isStreaming,
    isPageLike,
    host: parsed.host,
  };
}

export function suggestedCategory(info: UrlFileInfo): TagId {
  return info.valid ? info.category : "other";
}

export function detectFileNameFromUrl(raw: string): string | null {
  return inspectUrl(raw).fileName;
}

export function isDirectFile(info: UrlFileInfo): boolean {
  return (
    info.valid &&
    info.fileName !== null &&
    !info.isStreaming &&
    !info.isPageLike
  );
}

export function urlKey(raw: string): string {
  const info = inspectUrl(raw);
  if (!info.valid || info.url === null) return raw.trim().toLowerCase();
  const url = new URL(info.url.href);
  url.hash = "";
  return url.href.toLowerCase();
}

export function normalizeClipboardUrl(text: string): string | null {
  const firstLine = text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find((line) => line !== "");
  if (firstLine === undefined) return null;
  const info = inspectUrl(firstLine);
  return info.valid && info.url !== null ? info.url.href : null;
}
