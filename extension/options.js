/* RAPIDS extension options: local storage only. */
(function () {
  "use strict";

  var OPTIONS_KEY = "rapids.options.v1";
  var DEFAULT_THRESHOLD = 1;
  var form = document.getElementById("options-form");
  var threshold = document.getElementById("threshold");
  var autoScan = document.getElementById("auto-scan");
  var saveStatus = document.getElementById("save-status");
  var testButton = document.getElementById("test");
  var testStatus = document.getElementById("test-status");
  var version = document.getElementById("version");

  function normalizeThreshold(value) {
    var number = Number(value);
    if (!Number.isInteger(number) || number < 1 || number > 1024) {
      return DEFAULT_THRESHOLD;
    }
    return number;
  }

  function errorText(error) {
    if (error instanceof Error && error.message !== "") return error.message;
    return "The request did not complete.";
  }

  function setStatus(element, message) {
    element.textContent = message;
    element.style.color =
      message.toLowerCase().indexOf("could not") === 0 ? "#fca5a5" : "";
  }

  function storageGet(key) {
    return new Promise(function (resolve, reject) {
      try {
        chrome.storage.local.get(key).then(resolve, reject);
      } catch (error) {
        reject(error);
      }
    });
  }

  function storageSet(value) {
    return new Promise(function (resolve, reject) {
      try {
        chrome.storage.local.set(value).then(resolve, reject);
      } catch (error) {
        reject(error);
      }
    });
  }

  function sendMessage(message) {
    return new Promise(function (resolve, reject) {
      try {
        chrome.runtime.sendMessage(message).then(resolve, reject);
      } catch (error) {
        reject(error);
      }
    });
  }

  function load() {
    return storageGet(OPTIONS_KEY).then(function (stored) {
      var options = stored[OPTIONS_KEY];
      var value =
        options !== null && typeof options === "object"
          ? options.thresholdMb
          : undefined;
      threshold.value = String(normalizeThreshold(value));
      autoScan.checked =
        options !== null &&
        typeof options === "object" &&
        options.autoScan === true;
    });
  }

  function save(event) {
    event.preventDefault();
    var value = Number(threshold.value);
    if (!Number.isInteger(value) || value < 1 || value > 1024) {
      setStatus(
        saveStatus,
        "Could not save: use a whole number from 1 to 1024 MB.",
      );
      return;
    }
    storageSet({
      [OPTIONS_KEY]: {
        thresholdMb: value,
        autoScan: autoScan.checked,
      },
    }).then(
      function () {
        setStatus(
          saveStatus,
          "Saved. New responses use the updated threshold.",
        );
      },
      function (error) {
        setStatus(saveStatus, "Could not save: " + errorText(error));
      },
    );
  }

  function testConnection() {
    testButton.disabled = true;
    setStatus(testStatus, "Sending a handoff to RAPIDS...");
    sendMessage({ type: "rapids:test" }).then(
      function (reply) {
        testButton.disabled = false;
        if (reply !== null && reply !== undefined && reply.ok === true) {
          setStatus(
            testStatus,
            "Handoff sent. Accept the browser prompt and check the RAPIDS Add download sheet.",
          );
          return;
        }
        setStatus(
          testStatus,
          "Could not send the test handoff. " +
            (reply !== null &&
            reply !== undefined &&
            typeof reply.error === "string"
              ? reply.error
              : "Reload the extension and try again."),
        );
      },
      function (error) {
        testButton.disabled = false;
        setStatus(
          testStatus,
          "Could not send the test handoff. " + errorText(error),
        );
      },
    );
  }

  form.addEventListener("submit", save);
  testButton.addEventListener("click", testConnection);
  version.textContent = "v" + chrome.runtime.getManifest().version;
  void load().catch(function (error) {
    setStatus(saveStatus, "Could not read settings: " + errorText(error));
  });
})();
