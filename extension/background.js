/* RAPIDS MV3 service worker: sniff, store, badge, menus, and handoff. */
importScripts("handoff.js");

var STORAGE_KEY = "rapids.candidates.v1";
var OPTIONS_KEY = "rapids.options.v1";
var PENDING_TABS_KEY = "rapids.pendingTabs.v1";
var TTL_MS = 30 * 60 * 1000;
var PER_TAB_CAP = 200;
var TOTAL_CAP = 2000;
var FALLBACK_CLOSE_MS = 20000;
var TEST_URL =
  "https://download.blender.org/demo/movies/ToS/tears_of_steel_720p.mov";
var DEFAULT_OPTIONS = Object.freeze({ thresholdMb: 1, autoScan: false });
var RESTRICTED_HOSTS = new Set([
  "chromewebstore.google.com",
  "chrome.google.com",
  "microsoftedge.microsoft.com",
]);
var BINARY_TYPES = new Set([
  "application/octet-stream",
  "application/x-matroska",
  "application/vnd.rar",
  "application/zip",
  "application/x-7z-compressed",
  "application/pdf",
  "application/x-msdownload",
]);

var options = {
  thresholdMb: DEFAULT_OPTIONS.thresholdMb,
  autoScan: DEFAULT_OPTIONS.autoScan,
};
var storageQueue = Promise.resolve();
var cleanupTimers = new Map();

function emptyState() {
  return { version: 1, tabs: {} };
}

function normalizeOptions(value) {
  var rawThreshold =
    value !== null && typeof value === "object" ? value.thresholdMb : NaN;
  var threshold = Number(rawThreshold);
  if (!Number.isFinite(threshold)) threshold = DEFAULT_OPTIONS.thresholdMb;
  threshold = Math.min(1024, Math.max(1, Math.round(threshold)));
  return {
    thresholdMb: threshold,
    autoScan:
      value !== null && typeof value === "object" && value.autoScan === true,
  };
}

function isStoredItem(value) {
  return (
    value !== null &&
    typeof value === "object" &&
    typeof value.url === "string" &&
    globalThis.RapidsHandoff.normalizeHttpUrl(value.url) !== null
  );
}

function normalizeState(value) {
  if (
    value === null ||
    typeof value !== "object" ||
    typeof value.tabs !== "object"
  ) {
    return emptyState();
  }
  var tabs = {};
  var now = Date.now();
  Object.keys(value.tabs).forEach(function (key) {
    var tabId = Number(key);
    var bucket = value.tabs[key];
    if (
      !Number.isInteger(tabId) ||
      tabId < 0 ||
      bucket === null ||
      typeof bucket !== "object"
    ) {
      return;
    }
    if (!Array.isArray(bucket.items)) return;
    var items = bucket.items
      .filter(isStoredItem)
      .map(function (item) {
        var url = globalThis.RapidsHandoff.normalizeHttpUrl(item.url);
        if (url === null) return null;
        return {
          url: url,
          name: globalThis.RapidsHandoff.sanitizeName(item.name),
          size:
            Number.isSafeInteger(item.size) && item.size > 0 ? item.size : null,
          kind: globalThis.RapidsHandoff.normalizeKind(
            item.kind,
            item.contentType,
            url,
          ),
          contentType:
            typeof item.contentType === "string"
              ? item.contentType.slice(0, 120)
              : "",
          source:
            item.source === "dom" || item.source === "menu"
              ? item.source
              : "network",
          firstSeen: Number.isFinite(item.firstSeen) ? item.firstSeen : now,
          lastSeen: Number.isFinite(item.lastSeen) ? item.lastSeen : now,
          sentAt: Number.isFinite(item.sentAt) ? item.sentAt : null,
        };
      })
      .filter(function (item) {
        return item !== null;
      });
    if (items.length === 0) return;
    tabs[String(tabId)] = {
      pageUrl: globalThis.RapidsHandoff.normalizeHttpUrl(bucket.pageUrl) || "",
      updatedAt: Number.isFinite(bucket.updatedAt) ? bucket.updatedAt : now,
      items: items,
    };
  });
  return { version: 1, tabs: tabs };
}

function pruneExpired(state, now) {
  var changed = false;
  Object.keys(state.tabs).forEach(function (key) {
    var bucket = state.tabs[key];
    var items = bucket.items.filter(function (item) {
      return now - item.lastSeen < TTL_MS;
    });
    if (items.length !== bucket.items.length) {
      bucket.items = items;
      changed = true;
    }
    if (items.length === 0) {
      delete state.tabs[key];
      changed = true;
    }
  });
  return changed;
}

function capState(state, now) {
  pruneExpired(state, now);
  Object.keys(state.tabs).forEach(function (key) {
    var bucket = state.tabs[key];
    if (bucket.items.length > PER_TAB_CAP) {
      bucket.items.sort(function (left, right) {
        return right.lastSeen - left.lastSeen;
      });
      bucket.items = bucket.items.slice(0, PER_TAB_CAP);
    }
    if (bucket.items.length === 0) delete state.tabs[key];
  });

  var all = [];
  Object.keys(state.tabs).forEach(function (key) {
    state.tabs[key].items.forEach(function (item) {
      all.push({ tabKey: key, item: item });
    });
  });
  if (all.length > TOTAL_CAP) {
    all.sort(function (left, right) {
      return left.item.lastSeen - right.item.lastSeen;
    });
    var drop = new Set(
      all.slice(0, all.length - TOTAL_CAP).map(function (entry) {
        return entry.item;
      }),
    );
    Object.keys(state.tabs).forEach(function (key) {
      var bucket = state.tabs[key];
      bucket.items = bucket.items.filter(function (item) {
        return !drop.has(item);
      });
      if (bucket.items.length === 0) delete state.tabs[key];
    });
  }
}

function queueState(operation) {
  var task = storageQueue.then(function () {
    return chrome.storage.local.get(STORAGE_KEY).then(function (stored) {
      var state = normalizeState(stored[STORAGE_KEY]);
      var now = Date.now();
      pruneExpired(state, now);
      var outcome = operation(state, now);
      if (outcome.changed) {
        capState(state, now);
        return chrome.storage.local
          .set({ [STORAGE_KEY]: state })
          .then(function () {
            return outcome.value;
          });
      }
      return outcome.value;
    });
  });
  storageQueue = task.then(
    function () {
      return undefined;
    },
    function () {
      return undefined;
    },
  );
  return task;
}

function setBadge(tabId, count) {
  var text = count <= 0 ? "" : count > 99 ? "99+" : String(count);
  return chrome.action
    .setBadgeText({ tabId: tabId, text: text })
    .catch(function () {
      // The tab may have closed between the storage read and the badge write.
    });
}

function readTabSnapshot(tabId) {
  return queueState(function (state) {
    var bucket = state.tabs[String(tabId)];
    if (bucket === undefined) {
      return { changed: false, value: { pageUrl: "", items: [] } };
    }
    var items = bucket.items
      .slice()
      .sort(function (left, right) {
        return right.lastSeen - left.lastSeen;
      })
      .map(function (item) {
        return Object.assign({}, item);
      });
    return {
      changed: false,
      value: { pageUrl: bucket.pageUrl, items: items },
    };
  });
}

function toPopupItem(item) {
  return {
    url: item.url,
    name: item.name,
    size: item.size,
    kind: item.kind,
    sent: item.sentAt !== null,
  };
}

function mergeCandidates(tabId, pageUrl, candidates, source) {
  if (!Number.isInteger(tabId) || tabId < 0 || !Array.isArray(candidates)) {
    return Promise.resolve({ added: 0, count: 0 });
  }
  var now = Date.now();
  var prepared = [];
  var seen = new Set();
  candidates.slice(0, 300).forEach(function (raw) {
    if (raw === null || typeof raw !== "object") return;
    var url = globalThis.RapidsHandoff.normalizeHttpUrl(raw.url);
    if (url === null) return;
    var key = globalThis.RapidsHandoff.dedupeKey(url);
    if (key === null || seen.has(key)) return;
    seen.add(key);
    prepared.push({
      url: url,
      name:
        globalThis.RapidsHandoff.sanitizeName(raw.name) ||
        globalThis.RapidsHandoff.fileNameFromUrl(url),
      size: Number.isSafeInteger(raw.size) && raw.size > 0 ? raw.size : null,
      kind: globalThis.RapidsHandoff.normalizeKind(
        raw.kind,
        raw.contentType,
        url,
      ),
      contentType:
        typeof raw.contentType === "string"
          ? raw.contentType.slice(0, 120)
          : "",
      source: source,
      firstSeen: now,
      lastSeen: now,
      sentAt: null,
    });
  });
  if (prepared.length === 0) {
    return readTabSnapshot(tabId).then(function (snapshot) {
      return { added: 0, count: snapshot.items.length };
    });
  }
  var page = globalThis.RapidsHandoff.normalizeHttpUrl(pageUrl) || "";
  return queueState(function (state, at) {
    var key = String(tabId);
    var bucket = state.tabs[key];
    if (bucket === undefined) {
      bucket = { pageUrl: page, updatedAt: at, items: [] };
      state.tabs[key] = bucket;
    } else if (page !== "" && bucket.pageUrl !== page) {
      bucket.pageUrl = page;
    }
    var added = 0;
    prepared.forEach(function (candidate) {
      var candidateKey = globalThis.RapidsHandoff.dedupeKey(candidate.url);
      var existing = bucket.items.find(function (item) {
        return globalThis.RapidsHandoff.dedupeKey(item.url) === candidateKey;
      });
      if (existing === undefined) {
        bucket.items.push(candidate);
        added += 1;
        return;
      }
      if (existing.name === null && candidate.name !== null) {
        existing.name = candidate.name;
      }
      if (existing.size === null && candidate.size !== null) {
        existing.size = candidate.size;
      }
      if (existing.kind === "file" && candidate.kind !== "file") {
        existing.kind = candidate.kind;
      }
      if (existing.contentType === "" && candidate.contentType !== "") {
        existing.contentType = candidate.contentType;
      }
      existing.lastSeen = at;
    });
    bucket.updatedAt = at;
    capState(state, at);
    return {
      changed: true,
      value: {
        added: added,
        count: state.tabs[key] === undefined ? 0 : state.tabs[key].items.length,
      },
    };
  }).then(function (result) {
    if (result.added > 0) {
      void setBadge(tabId, result.count);
    }
    return result;
  });
}

function contentLength(value) {
  if (typeof value !== "string" || !/^\d+$/.test(value)) return null;
  var length = Number(value);
  return Number.isSafeInteger(length) && length > 0 ? length : null;
}

function isRestrictedHost(url) {
  return RESTRICTED_HOSTS.has(url.hostname.toLowerCase());
}

function canUseContent(tab) {
  if (tab === null || tab === undefined || typeof tab.id !== "number")
    return false;
  if (typeof tab.url !== "string") return false;
  try {
    var url = new URL(tab.url);
    return (
      (url.protocol === "http:" || url.protocol === "https:") &&
      !isRestrictedHost(url)
    );
  } catch {
    return false;
  }
}

function handleCompleted(details) {
  if (details.tabId < 0) return Promise.resolve();
  var url = globalThis.RapidsHandoff.normalizeHttpUrl(details.url);
  if (url === null) return Promise.resolve();

  var headers = {};
  (details.responseHeaders || []).forEach(function (header) {
    if (typeof header.name === "string" && typeof header.value === "string") {
      headers[header.name.toLowerCase()] = header.value;
    }
  });
  var contentType = (headers["content-type"] || "")
    .split(";")[0]
    .trim()
    .toLowerCase();
  var length = contentLength(headers["content-length"]);
  var threshold = options.thresholdMb * 1024 * 1024;
  var media =
    contentType.indexOf("video/") === 0 || contentType.indexOf("audio/") === 0;
  var binary =
    BINARY_TYPES.has(contentType) &&
    globalThis.RapidsHandoff.isDirectCandidateUrl(url);
  var large = length !== null && length >= threshold;
  if (!media && !binary && !large) return Promise.resolve();

  return chrome.tabs
    .get(details.tabId)
    .then(function (tab) {
      return mergeCandidates(
        details.tabId,
        tab !== undefined && tab.url ? tab.url : "",
        [
          {
            url: url,
            name:
              globalThis.RapidsHandoff.fileNameFromDisposition(
                headers["content-disposition"] || "",
              ) || globalThis.RapidsHandoff.fileNameFromUrl(url),
            size: length,
            kind: globalThis.RapidsHandoff.kindFrom(contentType, url),
            contentType: contentType,
          },
        ],
        "network",
      );
    })
    .catch(function () {
      // A tab can disappear while its response is being inspected.
    });
}

function markSent(tabId, url) {
  var key = globalThis.RapidsHandoff.dedupeKey(url);
  if (key === null) return Promise.resolve();
  return queueState(function (state, now) {
    var bucket = state.tabs[String(tabId)];
    if (bucket === undefined) return { changed: false, value: null };
    var item = bucket.items.find(function (entry) {
      return globalThis.RapidsHandoff.dedupeKey(entry.url) === key;
    });
    if (item === undefined) return { changed: false, value: null };
    item.sentAt = now;
    item.lastSeen = Math.max(item.lastSeen, now);
    bucket.updatedAt = now;
    return { changed: true, value: null };
  });
}

function notify(message) {
  return chrome.notifications
    .create({
      type: "basic",
      iconUrl: "icons/icon128.png",
      title: "RAPIDS",
      message: message,
    })
    .catch(function () {
      // The OS can disable notifications; the handoff still happened.
    });
}

function notifySent() {
  return notify("Sent to RAPIDS");
}

function sessionArea() {
  return chrome.storage.session !== undefined
    ? chrome.storage.session
    : chrome.storage.local;
}

function readPendingTabs() {
  return sessionArea()
    .get(PENDING_TABS_KEY)
    .then(function (stored) {
      var result = {};
      var raw = stored[PENDING_TABS_KEY];
      if (raw === null || typeof raw !== "object") return result;
      Object.keys(raw).forEach(function (key) {
        var tabId = Number(key);
        var entry = raw[key];
        if (!Number.isInteger(tabId) || tabId < 0) return;
        if (entry === null || typeof entry !== "object") return;
        if (!Number.isFinite(entry.deadline)) return;
        result[String(tabId)] = { deadline: entry.deadline };
      });
      return result;
    });
}

function writePendingTabs(pending) {
  return sessionArea().set({ [PENDING_TABS_KEY]: pending });
}

function rememberPendingTab(tabId) {
  return readPendingTabs()
    .then(function (pending) {
      pending[String(tabId)] = { deadline: Date.now() + FALLBACK_CLOSE_MS };
      return writePendingTabs(pending);
    })
    .catch(function () {
      // The timer still closes the tab when session storage is unavailable.
    });
}

function forgetPendingTab(tabId) {
  return readPendingTabs()
    .then(function (pending) {
      if (pending[String(tabId)] === undefined) return undefined;
      delete pending[String(tabId)];
      return writePendingTabs(pending);
    })
    .catch(function () {
      // Nothing to clean up.
    });
}

function closePendingTab(tabId) {
  return chrome.tabs
    .remove(tabId)
    .catch(function () {
      // The tab may already be gone.
    })
    .then(function () {
      var timer = cleanupTimers.get(tabId);
      if (timer !== undefined) {
        clearTimeout(timer);
        cleanupTimers.delete(tabId);
      }
      return forgetPendingTab(tabId);
    });
}

function scheduleTabCleanup(tabId, delay) {
  var existing = cleanupTimers.get(tabId);
  if (existing !== undefined) clearTimeout(existing);
  cleanupTimers.set(
    tabId,
    setTimeout(
      function () {
        cleanupTimers.delete(tabId);
        void closePendingTab(tabId);
      },
      Math.max(0, delay),
    ),
  );
}

function openExternalTab(rapidsUrl) {
  return chrome.tabs
    .create({ url: rapidsUrl, active: true })
    .then(function (tab) {
      if (tab.id !== undefined) {
        scheduleTabCleanup(tab.id, FALLBACK_CLOSE_MS);
        void rememberPendingTab(tab.id);
      }
      return tab;
    });
}

function handOffCandidate(candidate, tab) {
  var rapidsUrl;
  try {
    rapidsUrl = globalThis.RapidsHandoff.buildAddUrl(candidate);
  } catch (error) {
    return Promise.reject(error);
  }
  if (canUseContent(tab)) {
    return chrome.tabs
      .sendMessage(tab.id, { type: "rapids:handoff", rapidsUrl: rapidsUrl })
      .then(function (reply) {
        if (reply !== undefined && reply !== null && reply.ok === true) {
          return notifySent().then(function () {
            return { ok: true, method: "anchor" };
          });
        }
        return openExternalTab(rapidsUrl).then(function () {
          return notifySent().then(function () {
            return { ok: true, method: "tab" };
          });
        });
      })
      .catch(function () {
        return openExternalTab(rapidsUrl).then(function () {
          return notifySent().then(function () {
            return { ok: true, method: "tab" };
          });
        });
      });
  }
  return openExternalTab(rapidsUrl).then(function () {
    return notifySent().then(function () {
      return { ok: true, method: "tab" };
    });
  });
}

function popupPayload(tab) {
  return readTabSnapshot(tab.id).then(function (snapshot) {
    return {
      tabId: tab.id,
      pageUrl: snapshot.pageUrl || tab.url || "",
      supported: canUseContent(tab),
      items: snapshot.items.map(toPopupItem),
    };
  });
}

function popupState() {
  return chrome.tabs
    .query({ active: true, currentWindow: true })
    .then(function (tabs) {
      var tab = tabs[0];
      if (tab === undefined || tab.id === undefined) {
        return {
          ok: true,
          tabId: null,
          pageUrl: "",
          supported: false,
          items: [],
        };
      }
      return popupPayload(tab).then(function (payload) {
        void setBadge(tab.id, payload.items.length);
        return Object.assign({ ok: true }, payload);
      });
    });
}

function scanActiveTab() {
  return chrome.tabs
    .query({ active: true, currentWindow: true })
    .then(function (tabs) {
      var tab = tabs[0];
      if (tab === undefined || tab.id === undefined) {
        throw new Error("No active page is available.");
      }
      if (!canUseContent(tab)) {
        throw new Error(
          "The browser blocks page scans here. Use the context menu on a link instead.",
        );
      }
      return chrome.tabs
        .sendMessage(tab.id, { type: "rapids:scan-page" })
        .catch(function () {
          throw new Error("Reload the page once, then scan again.");
        })
        .then(function (reply) {
          if (
            reply === undefined ||
            reply === null ||
            reply.ok !== true ||
            !Array.isArray(reply.candidates)
          ) {
            throw new Error("The page did not return any links.");
          }
          return mergeCandidates(
            tab.id,
            tab.url || "",
            reply.candidates,
            "dom",
          ).then(function (result) {
            return popupPayload(tab).then(function (payload) {
              return Object.assign({ ok: true, added: result.added }, payload);
            });
          });
        });
    });
}

function sendUrl(rawUrl) {
  var key = globalThis.RapidsHandoff.dedupeKey(rawUrl);
  if (key === null) {
    return Promise.reject(new Error("Only HTTP or HTTPS links can be sent."));
  }
  return chrome.tabs
    .query({ active: true, currentWindow: true })
    .then(function (tabs) {
      var tab = tabs[0];
      if (tab === undefined || tab.id === undefined) {
        throw new Error("No active page is available.");
      }
      return readTabSnapshot(tab.id).then(function (snapshot) {
        var item = snapshot.items.find(function (entry) {
          return globalThis.RapidsHandoff.dedupeKey(entry.url) === key;
        });
        if (item === undefined) {
          throw new Error(
            "This download is no longer on the page. Scan again.",
          );
        }
        return handOffCandidate(
          {
            url: item.url,
            name: item.name,
            kind: item.kind,
            pageUrl: snapshot.pageUrl,
            size: item.size,
          },
          tab,
        ).then(function (result) {
          return markSent(tab.id, item.url).then(function () {
            return popupPayload(tab).then(function (payload) {
              return Object.assign(
                { ok: true, method: result.method },
                payload,
              );
            });
          });
        });
      });
    });
}

function mergeFromContent(message, sender) {
  if (sender.tab === undefined || typeof sender.tab.id !== "number") {
    return { ok: true, added: 0 };
  }
  return mergeCandidates(
    sender.tab.id,
    sender.tab.url || "",
    Array.isArray(message.candidates) ? message.candidates : [],
    "dom",
  ).then(function (result) {
    return { ok: true, added: result.added };
  });
}

function testConnection() {
  return handOffCandidate(
    {
      url: TEST_URL + "?rapids_test=" + Date.now(),
      name: "tears_of_steel_720p.mov",
      kind: "video",
      pageUrl: "",
      size: null,
    },
    null,
  );
}

function handleMessage(message, sender) {
  if (sender.id !== chrome.runtime.id) {
    return Promise.reject(new Error("Unauthorized RAPIDS request."));
  }
  if (message === null || typeof message !== "object") {
    return Promise.reject(new Error("Unknown RAPIDS request."));
  }
  switch (message.type) {
    case "rapids:get-state":
      return popupState();
    case "rapids:scan":
      return scanActiveTab();
    case "rapids:send":
      return sendUrl(typeof message.url === "string" ? message.url : "");
    case "rapids:test":
      return testConnection();
    case "rapids:dom-candidates":
      return mergeFromContent(message, sender);
    default:
      return Promise.reject(new Error("Unknown RAPIDS request."));
  }
}

function handleContextMenu(info, tab) {
  if (info.menuItemId !== "rapids-download") return Promise.resolve();
  var raw = info.linkUrl || info.srcUrl || "";
  var url = globalThis.RapidsHandoff.normalizeHttpUrl(raw);
  if (url === null) return Promise.resolve();
  var candidate = {
    url: url,
    name: globalThis.RapidsHandoff.fileNameFromUrl(url),
    kind: globalThis.RapidsHandoff.kindFrom("", url),
    pageUrl: tab !== undefined && tab.url ? tab.url : "",
    size: null,
  };
  return handOffCandidate(candidate, tab || null).catch(function () {
    return notify("Could not send this link to RAPIDS.");
  });
}

function handleTabNavigation(tabId, url) {
  var nextKey = globalThis.RapidsHandoff.dedupeKey(url) || "";
  return queueState(function (state) {
    var bucket = state.tabs[String(tabId)];
    if (bucket === undefined) return { changed: false, value: null };
    var currentKey = globalThis.RapidsHandoff.dedupeKey(bucket.pageUrl) || "";
    if (nextKey === "" || currentKey !== nextKey) {
      delete state.tabs[String(tabId)];
      return { changed: true, value: null };
    }
    return { changed: false, value: null };
  })
    .then(function () {
      return readTabSnapshot(tabId);
    })
    .then(function (snapshot) {
      return setBadge(tabId, snapshot.items.length);
    });
}

function handlePendingNavigation(tabId, url) {
  return readPendingTabs().then(function (pending) {
    var entry = pending[String(tabId)];
    if (entry === undefined) return undefined;
    if (url.indexOf("rapids://") === 0) return undefined;
    if (Date.now() < entry.deadline - 1000) return undefined;
    return closePendingTab(tabId);
  });
}

function installMenus() {
  chrome.contextMenus.removeAll(function () {
    void chrome.runtime.lastError;
    chrome.contextMenus.create(
      {
        id: "rapids-download",
        title: "Download with RAPIDS",
        contexts: ["link", "video", "audio"],
      },
      function () {
        void chrome.runtime.lastError;
      },
    );
  });
}

function loadOptions() {
  return chrome.storage.local.get(OPTIONS_KEY).then(function (stored) {
    options = normalizeOptions(stored[OPTIONS_KEY]);
  });
}

function ensureDefaults() {
  return chrome.storage.local.get(OPTIONS_KEY).then(function (stored) {
    if (stored[OPTIONS_KEY] === undefined) {
      return chrome.storage.local.set({
        [OPTIONS_KEY]: { ...DEFAULT_OPTIONS },
      });
    }
    return undefined;
  });
}

function restorePendingTabs() {
  return readPendingTabs().then(function (pending) {
    var now = Date.now();
    Object.keys(pending).forEach(function (key) {
      var tabId = Number(key);
      var delay = pending[key].deadline - now;
      if (delay <= 0) {
        void closePendingTab(tabId);
      } else {
        scheduleTabCleanup(tabId, delay);
      }
    });
  });
}

chrome.runtime.onInstalled.addListener(function () {
  installMenus();
  void ensureDefaults().catch(function () {
    // Defaults are also applied in memory.
  });
});

chrome.webRequest.onCompleted.addListener(
  function (details) {
    void handleCompleted(details).catch(function () {
      // Sniffing must never break browsing.
    });
  },
  { urls: ["<all_urls>"] },
  ["responseHeaders"],
);

chrome.contextMenus.onClicked.addListener(function (info, tab) {
  void handleContextMenu(info, tab).catch(function () {
    // The context menu path already surfaces its own failure notification.
  });
});

chrome.runtime.onMessage.addListener(function (message, sender, sendResponse) {
  void handleMessage(message, sender)
    .then(function (reply) {
      sendResponse(reply);
    })
    .catch(function (error) {
      sendResponse({
        ok: false,
        error:
          error instanceof Error && error.message !== ""
            ? error.message
            : "RAPIDS could not complete the request.",
      });
    });
  return true;
});

chrome.tabs.onRemoved.addListener(function (tabId) {
  void queueState(function (state) {
    if (state.tabs[String(tabId)] === undefined) {
      return { changed: false, value: null };
    }
    delete state.tabs[String(tabId)];
    return { changed: true, value: null };
  }).catch(function () {
    // Storage cleanup is best effort.
  });
  void forgetPendingTab(tabId);
});

chrome.tabs.onActivated.addListener(function (activeInfo) {
  void readTabSnapshot(activeInfo.tabId)
    .then(function (snapshot) {
      return setBadge(activeInfo.tabId, snapshot.items.length);
    })
    .catch(function () {
      // The tab may have closed immediately after activation.
    });
});

chrome.tabs.onUpdated.addListener(function (tabId, changeInfo) {
  if (changeInfo.url !== undefined) {
    void handleTabNavigation(tabId, changeInfo.url).catch(function () {
      // Navigation cleanup is best effort.
    });
    void handlePendingNavigation(tabId, changeInfo.url).catch(function () {
      // Fallback tab cleanup is best effort.
    });
    return;
  }
  if (changeInfo.status === "complete") {
    void readTabSnapshot(tabId)
      .then(function (snapshot) {
        return setBadge(tabId, snapshot.items.length);
      })
      .catch(function () {
        // The tab may have closed during loading.
      });
  }
});

chrome.storage.onChanged.addListener(function (changes, area) {
  if (area === "local" && changes[OPTIONS_KEY] !== undefined) {
    options = normalizeOptions(changes[OPTIONS_KEY].newValue);
  }
});

void chrome.action
  .setBadgeBackgroundColor({ color: "#1FB9DF" })
  .catch(function () {
    // Badge styling is cosmetic.
  });
void loadOptions().catch(function () {
  // The defaults are safe when storage is unavailable.
});
void restorePendingTabs().catch(function () {
  // A stale fallback tab is harmless; the timer path handles normal cleanup.
});
