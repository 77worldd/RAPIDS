/* RAPIDS DOM scanner and in-page handoff trigger. */
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

  function scan() {
    var candidates = [];
    var seen = new Set();
    var elements = document.querySelectorAll(
      "a[href], video, source[src], audio[src]",
    );
    for (var index = 0; index < elements.length; index += 1) {
      var element = elements[index];
      var raw;
      if (element.tagName === "A") {
        raw = element.href;
      } else if (element.tagName === "VIDEO" || element.tagName === "AUDIO") {
        raw = element.currentSrc || element.getAttribute("src") || "";
      } else {
        raw = element.getAttribute("src") || "";
      }
      raw = absolute(raw);
      var url = globalThis.RapidsHandoff.normalizeHttpUrl(raw);
      if (url === null || !globalThis.RapidsHandoff.isDirectCandidateUrl(url))
        continue;
      var key = globalThis.RapidsHandoff.dedupeKey(url);
      if (key === null || seen.has(key)) continue;
      seen.add(key);
      candidates.push({
        url: url,
        name: globalThis.RapidsHandoff.fileNameFromUrl(url),
        size: null,
        kind: globalThis.RapidsHandoff.kindFrom("", url),
      });
      if (candidates.length >= MAX_SCAN) break;
    }
    return candidates;
  }

  function sendCandidates() {
    var candidates = scan();
    if (candidates.length === 0) return;
    try {
      void chrome.runtime
        .sendMessage({
          type: "rapids:dom-candidates",
          candidates: candidates,
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
        sendResponse({ ok: true, candidates: scan() });
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
