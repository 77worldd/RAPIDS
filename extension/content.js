/* RAPIDS DOM scanner, page metadata capture, and in-page handoff trigger. */
(function () {
  "use strict";

  if (window.__rapidsContentLoaded === true) return;
  window.__rapidsContentLoaded = true;

  var MAX_SCAN = 300;
  var OPTIONS_KEY = "rapids.options.v1";

  function absolute(value) {
    if (typeof value !== "string" || value === "") return "";
    try {
      return new URL(value, document.baseURI).href;
    } catch {
      return "";
    }
  }

  function metaText(names) {
    for (var index = 0; index < names.length; index += 1) {
      var name = names[index];
      var node =
        document.querySelector('meta[property="' + name + '"]') ||
        document.querySelector('meta[name="' + name + '"]');
      if (node !== null) {
        var content = node.getAttribute("content") || "";
        if (content.trim() !== "") return content;
      }
    }
    return "";
  }

  function metaUrl(names) {
    return absolute(metaText(names));
  }

  function cleanText(value, limit) {
    var text = typeof value === "string" ? value : "";
    text = text.replace(/\s+/g, " ").trim();
    return text.length > limit ? text.slice(0, limit).trim() : text;
  }

  function pageMetadata() {
    var siteName = cleanText(metaText(["og:site_name"]), 120);
    return {
      pageUrl: location.href,
      pageTitle: cleanText(document.title, 300),
      siteName: siteName || location.hostname,
      ogTitle: cleanText(metaText(["og:title"]), 300),
      twitterTitle: cleanText(metaText(["twitter:title"]), 300),
      ogImage: metaUrl(["og:image", "og:image:url"]),
      twitterImage: metaUrl(["twitter:image", "twitter:image:src"]),
    };
  }

  function titleOf(media) {
    if (media === null) return "";
    var title =
      media.getAttribute("title") || media.getAttribute("aria-label") || "";
    return cleanText(title, 300);
  }

  function posterOf(media) {
    if (media === null || media.tagName !== "VIDEO") return "";
    return absolute(media.getAttribute("poster") || "");
  }

  function ownsUrl(media, url) {
    if (media === null) return false;
    var direct = absolute(media.getAttribute("src") || "");
    var current = absolute(media.currentSrc || "");
    return url === direct || url === current;
  }

  function heightOf(media, url) {
    if (media === null || media.tagName !== "VIDEO" || !ownsUrl(media, url)) {
      return null;
    }
    var height = media.videoHeight;
    if (!Number.isFinite(height) || height < 144) return null;
    return Math.round(height);
  }

  function durationOf(media, url) {
    if (media === null || !ownsUrl(media, url)) return null;
    var duration = media.duration;
    if (!Number.isFinite(duration) || duration <= 0) return null;
    return Math.round(duration * 10) / 10;
  }

  function scan() {
    var candidates = [];
    var seen = new Set();
    var elements = document.querySelectorAll(
      "a[href], video, audio, source[src]",
    );
    for (var index = 0; index < elements.length; index += 1) {
      var element = elements[index];
      var media = null;
      var raw;
      if (element.tagName === "A") {
        raw = element.href;
      } else if (element.tagName === "VIDEO" || element.tagName === "AUDIO") {
        media = element;
        raw = element.currentSrc || element.getAttribute("src") || "";
      } else {
        media = element.closest("video, audio");
        raw = element.getAttribute("src") || "";
      }
      raw = absolute(raw);
      var url = globalThis.RapidsHandoff.normalizeHttpUrl(raw);
      if (url === null || !globalThis.RapidsHandoff.isDirectCandidateUrl(url)) {
        continue;
      }
      var key = globalThis.RapidsHandoff.dedupeKey(url);
      if (key === null || seen.has(key)) continue;
      seen.add(key);
      candidates.push({
        url: url,
        name: globalThis.RapidsHandoff.fileNameFromUrl(url),
        size: null,
        kind: globalThis.RapidsHandoff.kindFrom("", url),
        title: titleOf(media),
        poster: posterOf(media),
        height: heightOf(media, url),
        duration: durationOf(media, url),
      });
      if (candidates.length >= MAX_SCAN) break;
    }
    return candidates;
  }

  function snapshot() {
    return { candidates: scan(), page: pageMetadata() };
  }

  function sendCandidates() {
    var state = snapshot();
    if (state.candidates.length === 0) return;
    try {
      void chrome.runtime
        .sendMessage({
          type: "rapids:dom-candidates",
          candidates: state.candidates,
          page: state.page,
        })
        .catch(function () {
          // The extension was reloaded or removed while this page stayed open.
        });
    } catch {
      // An invalidated extension context cannot message its worker.
    }
  }

  chrome.runtime.onMessage.addListener(
    function (message, _sender, sendResponse) {
      if (message === null || typeof message !== "object") return false;
      if (message.type === "rapids:scan-page") {
        sendResponse(Object.assign({ ok: true }, snapshot()));
        return false;
      }
      if (message.type === "rapids:handoff") {
        sendResponse({
          ok: globalThis.RapidsHandoff.triggerAnchor(message.rapidsUrl),
        });
        return false;
      }
      return false;
    },
  );

  function autoScanIfEnabled(options) {
    if (
      options !== null &&
      typeof options === "object" &&
      options.autoScan === true
    ) {
      sendCandidates();
    }
  }

  try {
    chrome.storage.local
      .get({ [OPTIONS_KEY]: { autoScan: false } })
      .then(function (stored) {
        autoScanIfEnabled(stored[OPTIONS_KEY]);
      })
      .catch(function () {
        // Defaults keep auto-scan off when storage is unavailable.
      });
  } catch {
    // The extension context may have been invalidated.
  }

  chrome.storage.onChanged.addListener(function (changes, area) {
    if (area !== "local" || changes[OPTIONS_KEY] === undefined) return;
    autoScanIfEnabled(changes[OPTIONS_KEY].newValue);
  });
})();
