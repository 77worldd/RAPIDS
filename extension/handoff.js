/*
 * RAPIDS browser handoff protocol.
 *
 * This file is intentionally dependency-free and context-free: the service
 * worker loads it with importScripts, and the content script receives it as a
 * normal content script. Replacing the deep link with Native Messaging later
 * means changing this module and background.js, not the sniffer or popup.
 */
(function (root) {
  "use strict";

  var MAX_URL_LENGTH = 4096;
  var MAX_NAME_LENGTH = 255;

  var VIDEO_EXTENSIONS = new Set([
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
  var AUDIO_EXTENSIONS = new Set([
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
  var FILE_EXTENSIONS = new Set([
    "zip",
    "rar",
    "7z",
    "tar",
    "gz",
    "bz2",
    "xz",
    "zst",
    "iso",
    "img",
    "exe",
    "msi",
    "msix",
    "appx",
    "apk",
    "dmg",
    "pkg",
    "deb",
    "rpm",
    "jar",
    "pdf",
    "epub",
    "mobi",
    "doc",
    "docx",
    "xls",
    "xlsx",
    "ppt",
    "pptx",
    "txt",
    "csv",
    "rtf",
    "odt",
    "ods",
  ]);

  function normalizeHttpUrl(value) {
    if (
      typeof value !== "string" ||
      value.length === 0 ||
      value.length > MAX_URL_LENGTH ||
      value.trim() !== value
    ) {
      return null;
    }
    try {
      var url = new URL(value);
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

  function sanitizeName(value) {
    if (typeof value !== "string") return null;
    var cleaned = "";
    for (var character of value) {
      var code = character.codePointAt(0);
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
    cleaned = cleaned.trim();
    if (cleaned === "" || [...cleaned].length > MAX_NAME_LENGTH) return null;
    return cleaned;
  }

  function lastPathSegment(value) {
    var url = normalizeHttpUrl(value);
    if (url === null) return null;
    try {
      var segments = new URL(url).pathname.split("/").filter(Boolean);
      var segment = segments[segments.length - 1];
      if (segment === undefined || segment === "") return null;
      try {
        segment = decodeURIComponent(segment);
      } catch {
        // Keep the encoded segment when it is not valid percent-encoding.
      }
      return sanitizeName(segment);
    } catch {
      return null;
    }
  }

  function fileNameFromUrl(value) {
    return lastPathSegment(value);
  }

  function extensionOf(value) {
    var segment = lastPathSegment(value);
    if (segment === null) return "";
    var dot = segment.lastIndexOf(".");
    if (dot <= 0 || dot === segment.length - 1) return "";
    return segment.slice(dot + 1).toLowerCase();
  }

  function isDirectCandidateUrl(value) {
    var url = normalizeHttpUrl(value);
    if (url === null) return false;
    var extension = extensionOf(url);
    return (
      VIDEO_EXTENSIONS.has(extension) ||
      AUDIO_EXTENSIONS.has(extension) ||
      FILE_EXTENSIONS.has(extension)
    );
  }

  function kindFrom(contentType, url) {
    var type = typeof contentType === "string" ? contentType.toLowerCase() : "";
    if (type.indexOf("video/") === 0) return "video";
    if (type.indexOf("audio/") === 0) return "audio";
    var extension = extensionOf(url);
    if (VIDEO_EXTENSIONS.has(extension)) return "video";
    if (AUDIO_EXTENSIONS.has(extension)) return "audio";
    return "file";
  }

  function normalizeKind(value, contentType, url) {
    if (value === "video" || value === "audio" || value === "file") {
      return value;
    }
    return kindFrom(contentType, url);
  }

  function fileNameFromDisposition(value) {
    if (typeof value !== "string" || value === "") return null;
    var extended = /filename\*\s*=\s*([^']*)'[^']*'([^;]+)/i.exec(value);
    if (extended !== null) {
      var encoded = extended[2].trim();
      if (
        encoded.charAt(0) === '"' &&
        encoded.charAt(encoded.length - 1) === '"'
      ) {
        encoded = encoded.slice(1, -1);
      }
      try {
        encoded = decodeURIComponent(encoded);
      } catch {
        // Fall back to the bytes as sent.
      }
      var decoded = sanitizeName(encoded);
      if (decoded !== null) return decoded;
    }
    var plain = /filename\s*=\s*("([^"]*)"|([^;]+))/i.exec(value);
    if (plain !== null) {
      var text = (plain[2] !== undefined ? plain[2] : plain[3]).trim();
      var name = sanitizeName(text);
      if (name !== null) return name;
    }
    return null;
  }

  function dedupeKey(value) {
    var url = normalizeHttpUrl(value);
    if (url === null) return null;
    var parsed = new URL(url);
    parsed.hash = "";
    return parsed.href;
  }

  function buildAddUrl(candidate) {
    if (candidate === null || typeof candidate !== "object") {
      throw new Error("The browser link is invalid.");
    }
    var url = normalizeHttpUrl(candidate.url);
    if (url === null) {
      throw new Error(
        "Only HTTP or HTTPS download links can be sent to RAPIDS.",
      );
    }

    var parameters = ["u=" + encodeURIComponent(url)];
    var name = sanitizeName(candidate.name);
    if (name === null) name = fileNameFromUrl(url);
    if (name !== null) {
      parameters.push("n=" + encodeURIComponent(name));
    }
    parameters.push(
      "t=" +
        encodeURIComponent(
          normalizeKind(candidate.kind, candidate.contentType, url),
        ),
    );
    var pageUrl = normalizeHttpUrl(candidate.pageUrl);
    if (pageUrl !== null) {
      parameters.push("p=" + encodeURIComponent(pageUrl));
    }
    if (Number.isSafeInteger(candidate.size) && candidate.size > 0) {
      parameters.push("sz=" + String(candidate.size));
    }
    parameters.push("v=1");

    var raw = "rapids://add?" + parameters.join("&");
    if (raw.length > MAX_URL_LENGTH) {
      throw new Error("This link is too long to send to RAPIDS.");
    }
    return raw;
  }

  function isRapidsAddUrl(value) {
    if (
      typeof value !== "string" ||
      value.length > MAX_URL_LENGTH ||
      value.indexOf("rapids://add?") !== 0
    ) {
      return false;
    }
    try {
      var url = new URL(value);
      return (
        url.protocol === "rapids:" &&
        url.hostname === "add" &&
        (url.pathname === "" || url.pathname === "/") &&
        url.searchParams.get("u") !== null
      );
    } catch {
      return false;
    }
  }

  /**
   * Synthetic anchor clicks are the primary handoff. A real page context keeps
   * the external-protocol prompt attached to the page the user was viewing.
   */
  function triggerAnchor(rapidsUrl) {
    if (typeof document === "undefined" || !isRapidsAddUrl(rapidsUrl)) {
      return false;
    }
    var anchor = document.createElement("a");
    anchor.href = rapidsUrl;
    anchor.rel = "noopener noreferrer";
    anchor.style.display = "none";
    var parent =
      document.body !== null ? document.body : document.documentElement;
    if (parent === null) return false;
    parent.appendChild(anchor);
    anchor.click();
    anchor.remove();
    return true;
  }

  root.RapidsHandoff = Object.freeze({
    MAX_URL_LENGTH: MAX_URL_LENGTH,
    buildAddUrl: buildAddUrl,
    dedupeKey: dedupeKey,
    extensionOf: extensionOf,
    fileNameFromDisposition: fileNameFromDisposition,
    fileNameFromUrl: fileNameFromUrl,
    isDirectCandidateUrl: isDirectCandidateUrl,
    isRapidsAddUrl: isRapidsAddUrl,
    kindFrom: kindFrom,
    normalizeHttpUrl: normalizeHttpUrl,
    normalizeKind: normalizeKind,
    sanitizeName: sanitizeName,
    triggerAnchor: triggerAnchor,
  });
})(globalThis);
