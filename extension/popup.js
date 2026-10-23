/* RAPIDS popup controller. All row content is inserted as text, never HTML. */
(function () {
  "use strict";

  var summary = document.getElementById("summary");
  var errorBox = document.getElementById("error");
  var itemsBox = document.getElementById("items");
  var scanButton = document.getElementById("scan");
  var hint = document.getElementById("hint");

  var state = {
    items: [],
    supported: false,
    pageUrl: "",
  };
  var scanning = false;

  function formatBytes(bytes) {
    if (!Number.isSafeInteger(bytes) || bytes <= 0) return "Size unknown";
    var units = ["B", "KB", "MB", "GB", "TB", "PB"];
    var value = bytes;
    var unit = 0;
    while (value >= 1024 && unit < units.length - 1) {
      value /= 1024;
      unit += 1;
    }
    if (unit === 0) return value + " B";
    return (
      value.toFixed(value >= 100 ? 0 : value >= 10 ? 1 : 2) + " " + units[unit]
    );
  }

  function hostOf(url) {
    try {
      return new URL(url).host;
    } catch {
      return "";
    }
  }

  function kindLabel(kind) {
    if (kind === "video") return "VIDEO";
    if (kind === "audio") return "AUDIO";
    return "FILE";
  }

  function errorText(error) {
    if (error instanceof Error && error.message !== "") return error.message;
    return "RAPIDS did not answer. Check that the desktop app is available.";
  }

  function setError(message) {
    errorBox.textContent = message;
    errorBox.hidden = message === "";
  }

  function isPopupItem(value) {
    if (value === null || typeof value !== "object") return false;
    if (typeof value.url !== "string") return false;
    try {
      var url = new URL(value.url);
      if (url.protocol !== "http:" && url.protocol !== "https:") return false;
    } catch {
      return false;
    }
    return (
      (value.kind === "video" ||
        value.kind === "audio" ||
        value.kind === "file") &&
      (value.size === null ||
        (Number.isSafeInteger(value.size) && value.size > 0)) &&
      (value.name === null || typeof value.name === "string") &&
      typeof value.sent === "boolean"
    );
  }

  function apply(reply) {
    state.items = Array.isArray(reply.items)
      ? reply.items.filter(isPopupItem)
      : [];
    state.supported = reply.supported === true;
    state.pageUrl = typeof reply.pageUrl === "string" ? reply.pageUrl : "";
    render();
  }

  function request(message) {
    return new Promise(function (resolve, reject) {
      try {
        chrome.runtime.sendMessage(message).then(resolve, reject);
      } catch (error) {
        reject(error);
      }
    }).then(
      function (reply) {
        if (reply === undefined || reply === null || reply.ok !== true) {
          var reason =
            reply !== null &&
            reply !== undefined &&
            typeof reply.error === "string" &&
            reply.error !== ""
              ? reply.error
              : "RAPIDS did not answer.";
          throw new Error(reason);
        }
        return reply;
      },
      function () {
        throw new Error(
          "The RAPIDS extension worker is unavailable. Reload the extension.",
        );
      },
    );
  }

  function makeItem(item) {
    var article = document.createElement("article");
    article.className = "item";

    var top = document.createElement("div");
    top.className = "item-top";

    var chip = document.createElement("span");
    chip.className = "chip chip-" + item.kind;
    chip.textContent = kindLabel(item.kind);

    var size = document.createElement("span");
    size.className = "size";
    size.textContent = formatBytes(item.size);

    top.appendChild(chip);
    top.appendChild(size);

    var name = document.createElement("p");
    name.className = "name";
    name.textContent =
      item.name !== null && item.name !== "" ? item.name : "Untitled download";
    name.title = item.name !== null && item.name !== "" ? item.name : item.url;

    var foot = document.createElement("div");
    foot.className = "item-foot";

    var host = document.createElement("span");
    host.className = "host";
    host.textContent = hostOf(item.url);
    host.title = item.url;

    var button = document.createElement("button");
    button.type = "button";
    button.className = item.sent ? "send sent" : "send";
    button.textContent = item.sent ? "✓ Sent" : "→ RAPIDS";
    button.disabled = item.sent;
    if (!item.sent) {
      button.addEventListener("click", function () {
        void sendItem(item, button);
      });
    }

    foot.appendChild(host);
    foot.appendChild(button);
    article.appendChild(top);
    article.appendChild(name);
    article.appendChild(foot);
    return article;
  }

  function emptyState() {
    var wrapper = document.createElement("div");
    wrapper.className = "empty";
    var mark = document.createElement("span");
    mark.className = "empty-mark";
    mark.textContent = "↓";
    var title = document.createElement("p");
    title.className = "empty-title";
    title.textContent = "No downloads found yet";
    var copy = document.createElement("p");
    copy.className = "empty-copy";
    copy.textContent = state.supported
      ? "Play a video, click a direct link, or run Scan this page. RAPIDS lists media and large files from this tab."
      : "The browser blocks scanning on this page. Right-click a link, video, or audio element and choose Download with RAPIDS.";
    wrapper.appendChild(mark);
    wrapper.appendChild(title);
    wrapper.appendChild(copy);
    return wrapper;
  }

  function render() {
    summary.textContent = state.items.length + " found on this page";
    scanButton.disabled = scanning || !state.supported;
    scanButton.querySelector("span").textContent = scanning
      ? "Scanning..."
      : "Scan this page";
    itemsBox.replaceChildren();
    if (state.items.length === 0) {
      itemsBox.appendChild(emptyState());
    } else {
      state.items.forEach(function (item) {
        itemsBox.appendChild(makeItem(item));
      });
    }
    if (!state.supported) {
      hint.textContent =
        "This page cannot host the scanner. Use the context menu on a link or media element to send it.";
    } else if (state.items.length === 0) {
      hint.textContent =
        "Auto-scan is off by default. Scan this page to collect direct media links, or right-click a link.";
    } else {
      hint.textContent = "";
    }
  }

  function load() {
    summary.textContent = "Checking this page...";
    setError("");
    return request({ type: "rapids:get-state" }).then(
      function (reply) {
        apply(reply);
      },
      function (error) {
        state.items = [];
        state.supported = false;
        summary.textContent = "RAPIDS is unavailable";
        setError(errorText(error));
        render();
      },
    );
  }

  function scan() {
    if (scanning) return;
    scanning = true;
    setError("");
    render();
    request({ type: "rapids:scan" }).then(
      function (reply) {
        scanning = false;
        apply(reply);
      },
      function (error) {
        scanning = false;
        setError(errorText(error));
        render();
      },
    );
  }

  function sendItem(item, button) {
    button.disabled = true;
    button.textContent = "Sending...";
    setError("");
    request({ type: "rapids:send", url: item.url }).then(
      function (reply) {
        apply(reply);
      },
      function (error) {
        button.disabled = false;
        button.textContent = "→ RAPIDS";
        setError(errorText(error));
      },
    );
  }

  scanButton.addEventListener("click", scan);
  void load();
})();
