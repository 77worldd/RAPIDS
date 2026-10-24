/* RAPIDS popup controller. All card content is inserted as text, never HTML. */
(function () {
  "use strict";

  var summary = document.getElementById("summary");
  var errorBox = document.getElementById("error");
  var itemsBox = document.getElementById("items");
  var scanButton = document.getElementById("scan");
  var hint = document.getElementById("hint");

  var state = {
    items: [],
    page: {},
    pageUrl: "",
    supported: false,
    groups: [],
    sent: new Map(),
  };
  var selected = Object.create(null);
  var scanning = false;
  var sending = false;
  var openMenuId = null;
  var pendingFocus = null;
  var pendingTriggerFocus = null;

  function formatBytes(bytes) {
    if (!Number.isSafeInteger(bytes) || bytes <= 0) return "";
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

  function formatDuration(seconds) {
    if (
      typeof seconds !== "number" ||
      !Number.isFinite(seconds) ||
      seconds <= 0
    ) {
      return "";
    }
    var total = Math.round(seconds);
    var hours = Math.floor(total / 3600);
    var minutes = Math.floor((total % 3600) / 60);
    var rest = total % 60;
    var padded = rest < 10 ? "0" + rest : String(rest);
    if (hours > 0) {
      var paddedMinutes = minutes < 10 ? "0" + minutes : String(minutes);
      return hours + ":" + paddedMinutes + ":" + padded;
    }
    return minutes + ":" + padded;
  }

  function kindLabel(kind) {
    if (kind === "video") return "VIDEO";
    if (kind === "audio") return "AUDIO";
    return "FILE";
  }

  function canonicalKey(url) {
    try {
      var key = globalThis.RapidsHandoff.dedupeKey(url);
      return key === null ? url : key;
    } catch {
      return url;
    }
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

  function buildGroups() {
    if (globalThis.RapidsMedia === undefined) return [];
    return globalThis.RapidsMedia.buildMediaItems(
      Object.assign(
        {
          pageUrl: state.pageUrl,
          candidates: state.items,
        },
        state.page,
      ),
    );
  }

  function pruneSelection() {
    Object.keys(selected).forEach(function (id) {
      var group = state.groups.find(function (entry) {
        return entry.id === id;
      });
      if (
        group === undefined ||
        !group.variants.some(function (variant) {
          return variant.url === selected[id];
        })
      ) {
        delete selected[id];
      }
    });
  }

  function apply(reply) {
    state.items = Array.isArray(reply.items)
      ? reply.items.filter(isPopupItem)
      : [];
    state.page =
      reply.page !== null && typeof reply.page === "object" ? reply.page : {};
    state.supported = reply.supported === true;
    state.pageUrl = typeof reply.pageUrl === "string" ? reply.pageUrl : "";
    state.sent = new Map();
    state.items.forEach(function (item) {
      state.sent.set(canonicalKey(item.url), item.sent === true);
    });
    state.groups = buildGroups();
    pruneSelection();
    if (
      openMenuId !== null &&
      !state.groups.some(function (group) {
        return group.id === openMenuId;
      })
    ) {
      openMenuId = null;
    }
    sending = false;
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

  function variantFor(item) {
    var stored = selected[item.id];
    if (typeof stored === "string") {
      var match = item.variants.find(function (variant) {
        return variant.url === stored;
      });
      if (match !== undefined) return match;
    }
    var preferred = item.variants.find(function (variant) {
      return variant.url === item.selectedVariantUrl;
    });
    return preferred === undefined ? item.variants[0] : preferred;
  }

  function sentFor(variant) {
    return state.sent.get(canonicalKey(variant.url)) === true;
  }

  function detailParts(variant) {
    var parts = [];
    if (variant.codec !== null) parts.push(variant.codec);
    if (variant.format !== "Unknown") parts.push(variant.format);
    var size = formatBytes(variant.size);
    if (size !== "") parts.push(size);
    var duration = formatDuration(variant.duration);
    if (duration !== "") parts.push(duration);
    if (variant.kind === "video") {
      if (variant.hasAudio === false) parts.push("No audio");
      else if (variant.hasAudio === true) parts.push("Audio included");
    }
    return parts;
  }

  function metaText(variant) {
    return [variant.label].concat(detailParts(variant)).join(" · ");
  }

  function optionDetail(variant) {
    var parts = detailParts(variant);
    if (sentFor(variant)) parts.push("Sent");
    return parts.join(" · ");
  }

  function thumbnailFallback() {
    var fallback = document.createElement("span");
    fallback.className = "thumb-fallback";
    var glyph = document.createElement("span");
    glyph.className = "thumb-glyph";
    fallback.appendChild(glyph);
    return fallback;
  }

  function makeThumb(item) {
    var thumb = document.createElement("div");
    thumb.className = "thumb";
    if (item.thumbnailUrl === null) {
      thumb.appendChild(thumbnailFallback());
      return thumb;
    }
    var image = document.createElement("img");
    image.alt = "";
    image.decoding = "async";
    image.loading = "lazy";
    image.referrerPolicy = "no-referrer";
    image.addEventListener(
      "error",
      function () {
        thumb.replaceChildren(thumbnailFallback());
      },
      { once: true },
    );
    image.src = item.thumbnailUrl;
    thumb.appendChild(image);
    return thumb;
  }

  function qualityLabel(count) {
    return count === 1 ? "1 quality" : count + " qualities";
  }

  function makeOption(item, variant, index) {
    var current = variantFor(item);
    var option = document.createElement("div");
    option.className = "quality-option";
    option.setAttribute("role", "option");
    option.setAttribute(
      "aria-selected",
      variant.url === current.url ? "true" : "false",
    );
    option.setAttribute("tabindex", "-1");
    option.dataset.index = String(index);

    var check = document.createElement("span");
    check.className = "option-check";
    check.setAttribute("aria-hidden", "true");
    check.textContent = variant.url === current.url ? "✓" : "";

    var label = document.createElement("span");
    label.className = "option-label";
    label.textContent = variant.label;

    var detail = document.createElement("span");
    detail.className = "option-detail";
    detail.textContent = optionDetail(variant);
    detail.title = detail.textContent;

    option.appendChild(check);
    option.appendChild(label);
    option.appendChild(detail);
    option.addEventListener("click", function () {
      chooseVariant(item, variant);
    });
    return option;
  }

  function makeMenu(item) {
    var menu = document.createElement("div");
    menu.className = "quality-menu";
    menu.setAttribute("role", "listbox");
    menu.setAttribute("aria-label", "Quality options for " + item.title);
    item.variants.forEach(function (variant, index) {
      menu.appendChild(makeOption(item, variant, index));
    });
    menu.addEventListener("keydown", onMenuKeyDown);
    return menu;
  }

  function makeCard(item) {
    var article = document.createElement("article");
    article.className = "media";
    article.dataset.itemId = item.id;

    var main = document.createElement("div");
    main.className = "media-main";
    main.appendChild(makeThumb(item));

    var body = document.createElement("div");
    body.className = "media-body";

    var top = document.createElement("div");
    top.className = "media-top";

    var title = document.createElement("h2");
    title.className = "title";
    title.textContent = item.title;
    title.title =
      item.title +
      " · " +
      item.host +
      " · " +
      qualityLabel(item.variants.length);

    var chip = document.createElement("span");
    chip.className = "chip";
    chip.textContent = kindLabel(item.kind);

    top.appendChild(title);
    top.appendChild(chip);

    var tech = document.createElement("p");
    tech.className = "tech";
    tech.textContent = item.technicalName;
    tech.title = item.technicalName;

    var variant = variantFor(item);
    var meta = document.createElement("p");
    meta.className = "meta";
    meta.textContent = metaText(variant);

    var actions = document.createElement("div");
    actions.className = "actions";

    var trigger = document.createElement("button");
    trigger.type = "button";
    trigger.className = "quality-trigger";
    trigger.setAttribute("aria-haspopup", "listbox");
    trigger.setAttribute(
      "aria-expanded",
      openMenuId === item.id ? "true" : "false",
    );
    trigger.setAttribute(
      "aria-label",
      "Quality for " + item.title + ": " + variant.label,
    );
    var triggerLabel = document.createElement("span");
    triggerLabel.className = "quality-label";
    triggerLabel.textContent = variant.label;
    var caret = document.createElement("span");
    caret.className = "caret";
    caret.setAttribute("aria-hidden", "true");
    trigger.appendChild(triggerLabel);
    trigger.appendChild(caret);
    trigger.addEventListener("click", function () {
      if (openMenuId === item.id) {
        closeMenu(true);
      } else {
        openMenu(item);
      }
    });
    trigger.addEventListener("keydown", function (event) {
      if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
      event.preventDefault();
      openMenu(item);
    });

    var sent = sentFor(variant);
    var sendButton = document.createElement("button");
    sendButton.type = "button";
    sendButton.className = sent ? "send sent" : "send";
    sendButton.textContent = sent ? "✓ Sent" : "Send";
    sendButton.disabled = sent;
    sendButton.setAttribute(
      "aria-label",
      (sent ? "Sent " : "Send ") + variant.label + " of " + item.title,
    );
    sendButton.addEventListener("click", function () {
      void sendVariant(item, variant, sendButton);
    });

    actions.appendChild(trigger);
    actions.appendChild(sendButton);

    body.appendChild(top);
    body.appendChild(tech);
    body.appendChild(meta);
    body.appendChild(actions);
    main.appendChild(body);
    article.appendChild(main);
    if (openMenuId === item.id) article.appendChild(makeMenu(item));
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
    title.textContent = "No media found yet";
    var copy = document.createElement("p");
    copy.className = "empty-copy";
    copy.textContent = state.supported
      ? "Play a video, click a direct link, or run Scan this page. RAPIDS groups matching streams into one item with a quality menu."
      : "The browser blocks scanning on this page. Right-click a link, video, or audio element and choose Download with RAPIDS.";
    wrapper.appendChild(mark);
    wrapper.appendChild(title);
    wrapper.appendChild(copy);
    return wrapper;
  }

  function render() {
    var count = state.groups.length;
    summary.textContent = scanning
      ? "Scanning this page..."
      : count === 0
        ? "No media found on this page"
        : count + (count === 1 ? " item" : " items") + " on this page";
    scanButton.disabled = scanning || !state.supported;
    scanButton.querySelector("span").textContent = scanning
      ? "Scanning..."
      : "Scan this page";
    itemsBox.replaceChildren();
    if (count === 0) {
      itemsBox.appendChild(emptyState());
    } else {
      state.groups.forEach(function (item) {
        itemsBox.appendChild(makeCard(item));
      });
    }
    if (!state.supported) {
      hint.textContent =
        "This page cannot host the scanner. Use the context menu on a link or media element to send it.";
    } else if (count === 0) {
      hint.textContent =
        "Auto-scan is off by default. Scan this page to collect direct media links, or right-click a link.";
    } else {
      hint.textContent = "";
    }
    if (pendingFocus !== null) {
      var card = itemsBox.querySelector(
        '[data-item-id="' + pendingFocus.itemId + '"]',
      );
      var option =
        card === null
          ? null
          : card.querySelector(
              '.quality-option[data-index="' + pendingFocus.index + '"]',
            );
      if (option !== null) option.focus();
      pendingFocus = null;
    }
    if (pendingTriggerFocus !== null) {
      var focusedCard = itemsBox.querySelector(
        '[data-item-id="' + pendingTriggerFocus + '"]',
      );
      var focusedTrigger =
        focusedCard === null
          ? null
          : focusedCard.querySelector(".quality-trigger");
      if (focusedTrigger !== null) focusedTrigger.focus();
      pendingTriggerFocus = null;
    }
  }

  function findGroup(id) {
    return state.groups.find(function (entry) {
      return entry.id === id;
    });
  }

  function openMenu(item) {
    openMenuId = item.id;
    var current = variantFor(item);
    var index = item.variants.findIndex(function (variant) {
      return variant.url === current.url;
    });
    pendingFocus = { itemId: item.id, index: index < 0 ? 0 : index };
    render();
  }

  function closeMenu(focusTrigger) {
    var id = openMenuId;
    openMenuId = null;
    if (focusTrigger && id !== null) pendingTriggerFocus = id;
    render();
  }

  function chooseVariant(item, variant) {
    selected[item.id] = variant.url;
    closeMenu(true);
  }

  function onMenuKeyDown(event) {
    var menu = event.currentTarget;
    var card = menu.closest(".media");
    var item = card === null ? undefined : findGroup(card.dataset.itemId);
    if (item === undefined) return;
    var options = Array.prototype.slice.call(
      menu.querySelectorAll(".quality-option"),
    );
    var current = options.indexOf(document.activeElement);
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      var step = event.key === "ArrowDown" ? 1 : -1;
      var next =
        current < 0
          ? step > 0
            ? 0
            : options.length - 1
          : (current + step + options.length) % options.length;
      options[next].focus();
      return;
    }
    if (event.key === "Home" || event.key === "End") {
      event.preventDefault();
      var edge = event.key === "Home" ? 0 : options.length - 1;
      options[edge].focus();
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      if (current >= 0) options[current].click();
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      closeMenu(true);
      return;
    }
    if (event.key === "Tab") {
      openMenuId = null;
      menu.hidden = true;
      var trigger = card.querySelector(".quality-trigger");
      if (trigger !== null) trigger.setAttribute("aria-expanded", "false");
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
        state.groups = [];
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
    openMenuId = null;
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

  function sendVariant(item, variant, button) {
    if (sending) return;
    sending = true;
    openMenuId = null;
    setError("");
    button.disabled = true;
    button.textContent = "Sending...";
    request({ type: "rapids:send", url: variant.url }).then(
      function (reply) {
        apply(reply);
      },
      function (error) {
        sending = false;
        setError(errorText(error));
        render();
      },
    );
  }

  document.addEventListener("click", function (event) {
    if (openMenuId === null) return;
    if (!(event.target instanceof Element)) return;
    if (
      event.target.closest(".quality-menu") !== null ||
      event.target.closest(".quality-trigger") !== null
    ) {
      return;
    }
    closeMenu(false);
  });

  scanButton.addEventListener("click", scan);
  void load();
})();
