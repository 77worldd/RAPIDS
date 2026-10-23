/*
 * Shared media grouping model. Kept in sync with src/lib/media.ts: the
 * extension has no build step, so the pure logic is mirrored rather than
 * bundled. Only page data is used; nothing here probes media bytes.
 */
(function (root) {
  "use strict";

  var MAX_URL_LENGTH = 4096;
  var MAX_NAME_LENGTH = 255;
  var MAX_VARIANTS = 8;
  var MAX_VARIANT_HEIGHT = 8192;
  var TITLE_LIMIT = 300;

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

  var FORMAT_LABELS = {
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

  var HEIGHT_TOKENS = new Map([
    ["8k", 4320],
    ["4k", 2160],
    ["2k", 1440],
    ["uhd", 2160],
    ["fhd", 1080],
    ["qhd", 1440],
    ["hd", 720],
    ["sd", 480],
  ]);

  var KNOWN_HEIGHTS = new Set([
    144, 240, 360, 480, 540, 576, 720, 1080, 1440, 2160, 4320,
  ]);

  var KNOWN_BITRATES = new Set([
    32, 48, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
  ]);

  var STRIPPED_TOKENS = new Set([
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

  var GENERIC_TITLES = new Set([
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

  function cleanText(value, limit) {
    limit = limit === undefined ? TITLE_LIMIT : limit;
    if (typeof value !== "string") return "";
    var cleaned = value
      .replace(/[\p{Cc}]+/gu, " ")
      .replace(/\s+/g, " ")
      .trim();
    return cleaned.length > limit ? cleaned.slice(0, limit).trim() : cleaned;
  }

  function sanitizeName(value) {
    if (typeof value !== "string") return null;
    var cleaned = "";
    for (var character of value) {
      var code = character.codePointAt(0) || 0;
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
    if (cleaned === "" || Array.from(cleaned).length > MAX_NAME_LENGTH) {
      return null;
    }
    return cleaned;
  }

  function parseHttpUrl(value) {
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

  function canonicalUrl(value) {
    var url = parseHttpUrl(value);
    if (url === null) return null;
    url.hash = "";
    return url.href;
  }

  function lastSegment(url) {
    var segments = url.pathname.split("/").filter(Boolean);
    var segment = segments[segments.length - 1];
    if (segment === undefined || segment === "") return null;
    try {
      return sanitizeName(decodeURIComponent(segment));
    } catch {
      return sanitizeName(segment);
    }
  }

  function stripExtension(value) {
    var name = sanitizeName(value) || "";
    var dot = name.lastIndexOf(".");
    if (dot <= 0 || dot === name.length - 1) return name;
    return name.slice(0, dot);
  }

  function extensionOf(value) {
    var name = sanitizeName(value) || "";
    var dot = name.lastIndexOf(".");
    if (dot <= 0 || dot === name.length - 1) return "";
    var extension = name.slice(dot + 1).toLowerCase();
    return /^[a-z0-9]{1,8}$/.test(extension) ? extension : "";
  }

  function formatOf(name) {
    var extension = extensionOf(name);
    if (extension === "") return "Unknown";
    return FORMAT_LABELS[extension] || extension.toUpperCase();
  }

  function kindFromContentType(contentType) {
    if (typeof contentType !== "string") return null;
    var type = contentType.toLowerCase();
    if (type.indexOf("video/") === 0) return "video";
    if (type.indexOf("audio/") === 0) return "audio";
    return null;
  }

  function kindFromName(name) {
    var extension = extensionOf(name);
    if (VIDEO_EXTENSIONS.has(extension)) return "video";
    if (AUDIO_EXTENSIONS.has(extension)) return "audio";
    return "file";
  }

  function validKind(value) {
    return value === "video" || value === "audio" || value === "file";
  }

  function validSource(value) {
    return (
      value === "video" ||
      value === "videoSource" ||
      value === "link" ||
      value === "navigation"
    );
  }

  function parseHeight(name) {
    var lower = name.toLowerCase();
    var dimensions =
      /(?:^|[^0-9])(\d{3,4})\s*[x×]\s*(\d{3,4})(?:[^0-9]|$)/.exec(lower);
    if (dimensions !== null) {
      var dimensionHeight = Number(dimensions[2]);
      if (Number.isSafeInteger(dimensionHeight) && dimensionHeight >= 144) {
        return dimensionHeight;
      }
    }
    var suffixed = /(?:^|[^0-9])(\d{3,4})p(?:[^a-z0-9]|$)/.exec(lower);
    if (suffixed !== null) {
      var suffixedHeight = Number(suffixed[1]);
      if (KNOWN_HEIGHTS.has(suffixedHeight)) return suffixedHeight;
      if (Number.isSafeInteger(suffixedHeight) && suffixedHeight >= 144) {
        return suffixedHeight;
      }
    }
    var tokens = lower.split(/[^a-z0-9]+/).filter(Boolean);
    for (var index = 0; index < tokens.length; index += 1) {
      var token = tokens[index];
      var known = HEIGHT_TOKENS.get(token);
      if (known !== undefined) return known;
      if (/^\d{3,4}$/.test(token)) {
        var height = Number(token);
        if (KNOWN_HEIGHTS.has(height)) return height;
      }
    }
    return null;
  }

  function explicitHeight(value) {
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

  function parseBitrate(name) {
    var unit =
      /(?:^|[^a-z0-9])(\d{2,4})(?:[._](\d+))?\s*(kbps|kbit|k|mbps|mbit|m|bps)(?=$|[^a-z0-9])/i.exec(
        name,
      );
    if (unit !== null) {
      var whole = Number(unit[1]);
      var fraction = unit[2] === undefined ? 0 : Number("0." + unit[2]);
      var unitText = unit[3] || "";
      var scale = unitText.toLowerCase().indexOf("m") === 0 ? 1000 : 1;
      var value = Math.round((whole + fraction) * scale);
      if (Number.isSafeInteger(value) && value > 0) return value;
    }
    var tokens = name
      .toLowerCase()
      .split(/[^a-z0-9]+/)
      .filter(Boolean);
    for (var index = 0; index < tokens.length; index += 1) {
      var token = tokens[index];
      if (!/^\d{2,4}$/.test(token)) continue;
      var bitrate = Number(token);
      if (KNOWN_BITRATES.has(bitrate)) return bitrate;
    }
    return null;
  }

  function parseCodec(name) {
    var lower = name.toLowerCase();
    if (/(?:^|[^a-z0-9])h\.?264(?:[^0-9]|$)|x264/.test(lower)) return "H.264";
    if (/(?:^|[^a-z0-9])h\.?265(?:[^0-9]|$)|x265|hevc/.test(lower)) {
      return "HEVC";
    }
    var tokens = lower.split(/[^a-z0-9]+/).filter(Boolean);
    for (var index = 0; index < tokens.length; index += 1) {
      var token = tokens[index];
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

  function parseHasAudio(name, kind) {
    if (kind === "audio") return true;
    var lower = name.toLowerCase().replace(/[._-]+/g, " ");
    if (/\b(video only|no audio|muted)\b/.test(lower)) return false;
    if (/\b(audio|with audio|aac|ac3|eac3|dts|opus)\b/.test(lower)) {
      return true;
    }
    return null;
  }

  function normalizeMediaStem(value) {
    var name = stripExtension(sanitizeName(value) || "");
    var lower = name
      .toLowerCase()
      .replace(/\b(?:video|audio)[ ._-]?only\b/g, " ")
      .replace(/\bno[ ._-]?audio\b/g, " ")
      .replace(/\bwith[ ._-]?audio\b/g, " ")
      .replace(
        /(?:^|[^a-z0-9])\d{2,4}(?:[._]\d+)?\s*(?:kbps|kbits?|k|mbps|mbits?|m|bps)(?=$|[^a-z0-9])/g,
        " ",
      );
    var rawTokens = lower.split(/[^a-z0-9]+/).filter(Boolean);
    var kept = rawTokens.filter(function (token) {
      return (
        !STRIPPED_TOKENS.has(token) &&
        !/^\d{2,3}fps$/.test(token) &&
        !KNOWN_HEIGHTS.has(Number(token)) &&
        !KNOWN_BITRATES.has(Number(token))
      );
    });
    return (kept.length > 0 ? kept : rawTokens).join(" ").trim();
  }

  function variantLabel(kind, height, bitrate) {
    if (kind === "audio") {
      return bitrate === null ? "Audio" : "Audio " + bitrate + " kbps";
    }
    if (height !== null) return height + "p";
    return "Original";
  }

  function compareVariants(left, right) {
    var leftHeight = left.height === null ? -1 : left.height;
    var rightHeight = right.height === null ? -1 : right.height;
    if (leftHeight !== rightHeight) return rightHeight - leftHeight;
    var leftBitrate = parseBitrate(left.label) || 0;
    var rightBitrate = parseBitrate(right.label) || 0;
    if (leftBitrate !== rightBitrate) return rightBitrate - leftBitrate;
    if (left.label !== right.label)
      return left.label.localeCompare(right.label);
    return left.url.localeCompare(right.url);
  }

  function selectDefaultVariant(variants) {
    if (variants.length === 0) return null;
    var sorted = variants.slice().sort(compareVariants);
    var known = sorted.filter(function (variant) {
      return variant.height !== null;
    });
    var below = known.filter(function (variant) {
      return (variant.height === null ? 0 : variant.height) <= 1080;
    });
    if (below.length > 0) return below[0];
    if (known.length > 0) return known[known.length - 1];
    for (var index = 0; index < sorted.length; index += 1) {
      if (sorted[index].kind !== "audio") return sorted[index];
    }
    return sorted[0] || null;
  }

  function cleanPageTitle(value, siteName) {
    var title = cleanText(value);
    if (title === "") return "";
    title = title.replace(/\s+(?:\||·|•)\s+.*$/, "").trim();
    var hyphen = /^(.*\S)\s+[—–-]\s+(.+)$/.exec(title);
    if (hyphen !== null) {
      var head = (hyphen[1] || "").trim();
      var tail = (hyphen[2] || "").trim();
      var tailLower = tail.toLowerCase();
      var siteLower = (siteName || "").toLowerCase();
      var looksLikeSite =
        (tailLower !== "" &&
          siteLower !== "" &&
          (tailLower === siteLower || tailLower.indexOf(siteLower) !== -1)) ||
        (!/\s/.test(tail) && tail.length <= 24);
      if (head !== "" && looksLikeSite) title = head;
    }
    return cleanText(title);
  }

  function humanizeName(value) {
    if (value === null) return "";
    return stripExtension(value)
      .replace(/[._-]+/g, " ")
      .replace(/\s+/g, " ")
      .trim();
  }

  function resolveMediaTitle(input) {
    var specific = cleanText(input.specific);
    if (specific !== "" && !GENERIC_TITLES.has(specific.toLowerCase())) {
      return specific;
    }
    var og = cleanText(input.ogTitle);
    if (og !== "" && !GENERIC_TITLES.has(og.toLowerCase())) return og;
    var twitter = cleanText(input.twitterTitle);
    if (twitter !== "" && !GENERIC_TITLES.has(twitter.toLowerCase())) {
      return twitter;
    }
    var siteName = cleanText(input.siteName);
    var page = cleanPageTitle(
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
    var file = humanizeName(
      input.fileName === undefined ? null : input.fileName,
    );
    if (file !== "" && !GENERIC_TITLES.has(file.toLowerCase())) return file;
    var parsedUrl = parseHttpUrl(input.url);
    var urlName = parsedUrl === null ? null : lastSegment(parsedUrl);
    var stem = normalizeMediaStem(urlName || "");
    if (stem !== "") return stem.charAt(0).toUpperCase() + stem.slice(1);
    return "Untitled media";
  }

  function mediaId(pageUrl, title, groupKey) {
    var value = pageUrl + "\n" + title.trim().toLowerCase() + "\n" + groupKey;
    var hash = 0x811c9dc5;
    for (var index = 0; index < value.length; index += 1) {
      hash ^= value.charCodeAt(index);
      hash = Math.imul(hash, 0x01000193) >>> 0;
    }
    return "media_" + hash.toString(16).padStart(8, "0");
  }

  function thumbnailFor(input) {
    var poster = canonicalUrl(input.poster);
    if (poster !== null) return { url: poster, kind: "poster" };
    var openGraph = canonicalUrl(input.ogImage);
    if (openGraph !== null) return { url: openGraph, kind: "og" };
    var twitter = canonicalUrl(input.twitterImage);
    if (twitter !== null) return { url: twitter, kind: "og" };
    return { url: null, kind: "none" };
  }

  function prepare(candidate) {
    var url = canonicalUrl(candidate.url);
    if (url === null) return null;
    var parsed = new URL(url);
    var name =
      sanitizeName(candidate.name) || lastSegment(parsed) || parsed.hostname;
    if (name === "") return null;
    var extension = extensionOf(name);
    if (extension === "m3u8" || extension === "mpd") return null;
    var kind = validKind(candidate.kind)
      ? candidate.kind
      : kindFromContentType(candidate.contentType) || kindFromName(name);
    var height = explicitHeight(candidate.height);
    if (height === null) height = parseHeight(name);
    var bitrate = kind === "audio" ? parseBitrate(name) : null;
    var size =
      typeof candidate.size === "number" &&
      Number.isSafeInteger(candidate.size) &&
      candidate.size > 0
        ? candidate.size
        : null;
    var hasAudio =
      typeof candidate.hasAudio === "boolean"
        ? candidate.hasAudio
        : parseHasAudio(name, kind);
    var family = kind === "file" ? "file" : "media";
    var stem = normalizeMediaStem(name);
    return {
      url: url,
      name: name,
      kind: kind,
      height: height,
      label: variantLabel(kind, height, bitrate),
      format: formatOf(name),
      size: size,
      hasAudio: hasAudio,
      codec: parseCodec(name),
      source: validSource(candidate.source) ? candidate.source : "link",
      title: cleanText(candidate.title),
      groupKey: family + "\u0000" + stem,
    };
  }

  function mergePrepared(left, right) {
    return {
      url: left.url,
      name: left.name,
      kind:
        left.kind === "file" && right.kind !== "file" ? right.kind : left.kind,
      height: left.height === null ? right.height : left.height,
      format: left.format,
      size: left.size === null ? right.size : left.size,
      hasAudio: left.hasAudio === null ? right.hasAudio : left.hasAudio,
      codec: left.codec === null ? right.codec : left.codec,
      source: left.source === "link" ? right.source : left.source,
      title: left.title !== "" ? left.title : right.title,
      groupKey: left.groupKey,
    };
  }

  function toVariant(prepared) {
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

  function buildMediaItems(input) {
    var pageUrl = canonicalUrl(input.pageUrl);
    if (pageUrl === null) return [];
    var page = new URL(pageUrl);
    var thumbnail = thumbnailFor(input);
    var unique = new Map();
    var candidates = Array.isArray(input.candidates) ? input.candidates : [];
    for (var index = 0; index < candidates.length; index += 1) {
      var prepared = prepare(candidates[index]);
      if (prepared === null) continue;
      var existing = unique.get(prepared.url);
      if (existing === undefined) {
        unique.set(prepared.url, prepared);
      } else {
        unique.set(prepared.url, mergePrepared(existing, prepared));
      }
    }

    var groups = new Map();
    unique.forEach(function (prepared) {
      var bucket = groups.get(prepared.groupKey);
      if (bucket === undefined) groups.set(prepared.groupKey, [prepared]);
      else bucket.push(prepared);
    });

    var items = [];
    groups.forEach(function (bucket, groupKey) {
      var variants = bucket
        .map(toVariant)
        .sort(compareVariants)
        .slice(0, MAX_VARIANTS);
      var selected = selectDefaultVariant(variants);
      if (selected === null) return;
      var chosen = null;
      for (var chosenIndex = 0; chosenIndex < bucket.length; chosenIndex += 1) {
        if (bucket[chosenIndex].url === selected.url) {
          chosen = bucket[chosenIndex];
          break;
        }
      }
      if (chosen === null) chosen = bucket[0];
      if (chosen === undefined) return;
      var specific = null;
      for (var titleIndex = 0; titleIndex < bucket.length; titleIndex += 1) {
        if (bucket[titleIndex].title !== "") {
          specific = bucket[titleIndex].title;
          break;
        }
      }
      var title = resolveMediaTitle({
        specific: specific,
        ogTitle: input.ogTitle,
        twitterTitle: input.twitterTitle,
        pageTitle: input.pageTitle,
        siteName: input.siteName,
        fileName: chosen.name,
        url: chosen.url,
      });
      var hasVideo = false;
      var hasAudio = false;
      for (var kindIndex = 0; kindIndex < variants.length; kindIndex += 1) {
        if (variants[kindIndex].kind === "video") hasVideo = true;
        if (variants[kindIndex].kind === "audio") hasAudio = true;
      }
      items.push({
        id: mediaId(pageUrl, title, groupKey),
        title: title,
        technicalName: chosen.name,
        pageUrl: pageUrl,
        host: page.host,
        kind: hasVideo ? "video" : hasAudio ? "audio" : "file",
        thumbnailUrl: thumbnail.url,
        thumbnailKind: thumbnail.kind,
        variants: variants,
        selectedVariantUrl: selected.url,
      });
    });
    return items;
  }

  root.RapidsMedia = Object.freeze({
    MAX_VARIANTS: MAX_VARIANTS,
    buildMediaItems: buildMediaItems,
    normalizeMediaStem: normalizeMediaStem,
    resolveMediaTitle: resolveMediaTitle,
    selectDefaultVariant: selectDefaultVariant,
  });
})(globalThis);
