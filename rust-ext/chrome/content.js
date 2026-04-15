// voice-typing — content script
// Overlays a state-aware icon on focused text inputs, reflecting the live
// state of the local ASR service running on the local machine.

(() => {
  "use strict";

  const api =
    typeof chrome !== "undefined" && chrome.runtime ? chrome : browser;

  let state = "disconnected"; // disconnected | idle | booting | active | error
  let overlay = null;
  let trackedInput = null;
  let bootingTimeout = null;
  let defsInjected = false;

  // ── Streamline SVG icons ──────────────────────────────────────────────

  const SVG_MIC =
    '<svg class="dd-icon" viewBox="0 0 14 14" xmlns="http://www.w3.org/2000/svg">' +
    '<path class="dd-p" fill-rule="evenodd" d="M6.75 0a2.946 2.946 0 0 0-2.946 2.946v3.425a2.946 2.946 0 1 0 5.892 0V2.946A2.946 2.946 0 0 0 6.75 0Z" clip-rule="evenodd"/>' +
    '<path class="dd-s" fill-rule="evenodd" d="M2.5 6.763a.75.75 0 1 0-1.5 0 5.25 5.25 0 0 0 5 5.245v1.212a.75.75 0 0 0 1.5 0v-1.213a5.25 5.25 0 0 0 5-5.244.75.75 0 0 0-1.5 0 3.75 3.75 0 0 1-3.75 3.75h-.486l-.014 0-.014 0H6.25a3.75 3.75 0 0 1-3.75-3.75Z" clip-rule="evenodd"/>' +
    "</svg>";

  const SVG_CLOCK =
    '<svg class="dd-icon dd-spin" viewBox="0 0 14 14" xmlns="http://www.w3.org/2000/svg">' +
    '<path class="dd-s" fill-rule="evenodd" d="M7 2.75a.75.75 0 0 1 .75.75v3.575l2.136 1.282a.75.75 0 1 1-.772 1.286l-2.5-1.5A.75.75 0 0 1 6.25 7.5v-4A.75.75 0 0 1 7 2.75Z" clip-rule="evenodd"/>' +
    '<path class="dd-p" fill-rule="evenodd" d="M1.5 7a5.5 5.5 0 0 1 9.82-3.405l-.966.965a.5.5 0 0 0 .353.854H13.5a.5.5 0 0 0 .5-.5V2.12a.5.5 0 0 0-.854-.354l-.76.761a7 7 0 1 0 1.427 6.086.75.75 0 0 0-1.46-.344A5.5 5.5 0 0 1 1.5 7Z" clip-rule="evenodd"/>' +
    "</svg>";

  const SVG_MUTED =
    '<svg class="dd-icon" viewBox="0 0 14 14" xmlns="http://www.w3.org/2000/svg">' +
    '<path class="dd-p" fill-rule="evenodd" d="M2.5 6.763a.75.75 0 1 0-1.5 0 5.25 5.25 0 0 0 5 5.245v1.212a.75.75 0 0 0 1.5 0v-1.213a5.25 5.25 0 0 0 5-5.244.75.75 0 0 0-1.5 0 3.75 3.75 0 0 1-3.75 3.75h-.486l-.014 0-.014 0H6.25a3.75 3.75 0 0 1-3.75-3.75Z" clip-rule="evenodd"/>' +
    '<path class="dd-p" fill-rule="evenodd" d="M6.75 0a2.946 2.946 0 0 0-2.946 2.946v3.425a2.946 2.946 0 1 0 5.892 0V2.946A2.946 2.946 0 0 0 6.75 0Z" clip-rule="evenodd"/>' +
    '<path class="dd-a" fill-rule="evenodd" d="M.22.22a.75.75 0 0 0 0 1.06l12.5 12.5a.75.75 0 1 0 1.06-1.06L1.28.22a.75.75 0 0 0-1.06 0Z" clip-rule="evenodd"/>' +
    "</svg>";

  const SVG_WARN =
    '<svg class="dd-icon" viewBox="0 0 14 14" xmlns="http://www.w3.org/2000/svg">' +
    '<path class="dd-p" fill-rule="evenodd" d="M7 .006a1.5 1.5 0 0 0-1.335.816l-.002.004-5.5 10.999A1.5 1.5 0 0 0 1.5 14h11.002a1.5 1.5 0 0 0 1.335-2.174L8.336.826A1.5 1.5 0 0 0 7 .006Z" clip-rule="evenodd"/>' +
    '<path class="dd-s" fill-rule="evenodd" d="M7.75 4.875a.75.75 0 0 0-1.5 0v3.25a.75.75 0 0 0 1.5 0v-3.25ZM7 11.875a1 1 0 1 0 0-2 1 1 0 0 0 0 2Z" clip-rule="evenodd"/>' +
    "</svg>";

  function iconForState(s) {
    switch (s) {
      case "idle":
      case "active":
        return SVG_MIC;
      case "booting":
        return SVG_CLOCK;
      case "error":
        return SVG_WARN;
      default:
        return SVG_MUTED; // disconnected
    }
  }

  // ── Gradient defs (injected once for active-state fill) ───────────────

  function ensureDefs() {
    if (defsInjected) return;
    defsInjected = true;
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("width", "0");
    svg.setAttribute("height", "0");
    svg.style.position = "absolute";
    svg.innerHTML =
      "<defs>" +
      '<linearGradient id="ddGrad" x1="0" y1="1" x2="0" y2="0">' +
      '<stop offset="0%" stop-color="#ff3333"/>' +
      '<stop offset="50%" stop-color="#9b30ff"/>' +
      '<stop offset="100%" stop-color="#ffffff"/>' +
      "</linearGradient>" +
      "</defs>";
    document.documentElement.appendChild(svg);
  }

  // ── Communication with background service worker ──────────────────────

  api.runtime.onMessage.addListener((msg) => {
    if (msg.type === "stateUpdate") {
      state = msg.state;
      clearBootingTimeout();
      rebuildOverlay();
    }
  });

  api.runtime.sendMessage({ type: "getState" }, (resp) => {
    if (api.runtime.lastError) return;
    if (resp && resp.state) {
      state = resp.state;
      rebuildOverlay();
    }
  });

  // ── Focus tracking ────────────────────────────────────────────────────

  document.addEventListener(
    "focusin",
    (e) => {
      if (isTextInput(e.target)) showOverlay(e.target);
    },
    true,
  );

  document.addEventListener(
    "focusout",
    () => {
      setTimeout(() => {
        if (
          overlay &&
          document.activeElement !== trackedInput &&
          !overlay.contains(document.activeElement)
        ) {
          hideOverlay();
        }
      }, 150);
    },
    true,
  );

  function isTextInput(el) {
    if (!el || !el.tagName) return false;
    const tag = el.tagName.toLowerCase();
    if (tag === "textarea") return true;
    if (tag === "input") {
      const t = (el.type || "text").toLowerCase();
      return ["text", "search", "email", "url", "tel"].includes(t);
    }
    return el.isContentEditable;
  }

  // ── Overlay lifecycle ─────────────────────────────────────────────────

  function showOverlay(input) {
    hideOverlay();
    trackedInput = input;

    const rect = input.getBoundingClientRect();
    if (rect.width < 48) return;

    ensureDefs();

    overlay = document.createElement("div");
    overlay.className = "voice-typing-overlay";
    renderOverlay();

    overlay.addEventListener("mousedown", onMicClick);
    document.documentElement.appendChild(overlay);
    positionOverlay();

    window.addEventListener("scroll", positionOverlay, true);
    window.addEventListener("resize", positionOverlay);
  }

  function hideOverlay() {
    window.removeEventListener("scroll", positionOverlay, true);
    window.removeEventListener("resize", positionOverlay);
    if (overlay) {
      overlay.removeEventListener("mousedown", onMicClick);
      overlay.remove();
      overlay = null;
    }
    trackedInput = null;
    clearBootingTimeout();
  }

  function renderOverlay() {
    if (!overlay) return;
    overlay.setAttribute("data-state", state);
    overlay.innerHTML =
      '<div class="voice-typing-arcs">' +
      '<div class="voice-typing-arc voice-typing-arc-1"></div>' +
      '<div class="voice-typing-arc voice-typing-arc-2"></div>' +
      '<div class="voice-typing-arc voice-typing-arc-3"></div>' +
      "</div>" +
      '<div class="voice-typing-icon">' +
      iconForState(state) +
      "</div>";
  }

  function rebuildOverlay() {
    if (!overlay) return;
    renderOverlay();
  }

  function positionOverlay() {
    if (!overlay || !trackedInput) return;
    const rect = trackedInput.getBoundingClientRect();
    const sz = 22;
    const pad = 6;
    overlay.style.top = rect.top + (rect.height - sz) / 2 + "px";
    overlay.style.left = rect.left + pad + "px";
  }

  // ── Click handling ────────────────────────────────────────────────────

  function onMicClick(e) {
    e.preventDefault();
    e.stopPropagation();

    if (state !== "active" && state !== "booting") {
      state = "booting";
      rebuildOverlay();
      bootingTimeout = setTimeout(() => {
        if (state === "booting") {
          state = "error";
          rebuildOverlay();
        }
      }, 8000);
    }

    api.runtime.sendMessage({ type: "toggle" });
    if (trackedInput) trackedInput.focus();
  }

  function clearBootingTimeout() {
    if (bootingTimeout) {
      clearTimeout(bootingTimeout);
      bootingTimeout = null;
    }
  }
})();
