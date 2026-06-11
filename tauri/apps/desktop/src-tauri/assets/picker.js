// Springtale selector picker — injected into the picker webview
// by `commands::selector_picker::open_selector_picker`. Hover to
// highlight, click to pick, Escape to cancel. Emits one
// `selector-picked` event with `{ selector, tag_name }`.
//
// Inlined `@medv/finder` for selector generation (MIT, 1.5KB
// minified). We bundle rather than CDN-load so the picker works
// offline + has no third-party request surface. Generation rules:
//   - Prefer `[id]` when present (unique by definition).
//   - Otherwise build the shortest `tag[.class]:nth-of-type` chain
//     that uniquely identifies the element.
//
// Spec: window.__SPRINGTALE_HOST_ALLOWLIST__ is the array of
// hosts the recipe declared allowed. We don't block navigation —
// the user controls the address bar — but we surface a warning
// chrome when the document host isn't in the list, so the user
// notices when they've navigated off-recipe.
(() => {
  if (window.__SPRINGTALE_PICKER_ACTIVE__) return;
  window.__SPRINGTALE_PICKER_ACTIVE__ = true;

  const HIGHLIGHT_OUTLINE = "2px solid #ff6680";
  const ALLOWLIST = window.__SPRINGTALE_HOST_ALLOWLIST__ || [];

  // ── @medv/finder inline (MIT) ────────────────────────────────
  // Minimal port — generates a stable, short CSS selector for a
  // DOM element. Avoids dynamic class names (those that look
  // generated). Tested against React / Vue / Solid output.
  function finder(element) {
    if (element.id) return `#${cssEscape(element.id)}`;
    const parts = [];
    let node = element;
    while (node && node.nodeType === 1 && node !== document.documentElement) {
      let part = node.tagName.toLowerCase();
      const stableClass = pickStableClass(node);
      if (stableClass) part += `.${cssEscape(stableClass)}`;
      const idx = nthOfTypeIndex(node);
      if (idx > 0) part += `:nth-of-type(${idx + 1})`;
      parts.unshift(part);
      // Stop early — most selectors are unique within 3 levels.
      const probe = parts.join(" > ");
      if (uniqueWithin(document, probe)) return probe;
      node = node.parentElement;
    }
    return parts.join(" > ");
  }

  function pickStableClass(el) {
    if (!el.className || typeof el.className !== "string") return null;
    for (const c of el.className.split(/\s+/).filter(Boolean)) {
      if (/^[a-z][a-z0-9_-]{2,}$/i.test(c) && !/_\d{4,}/.test(c)) {
        return c;
      }
    }
    return null;
  }

  function nthOfTypeIndex(el) {
    const parent = el.parentElement;
    if (!parent) return 0;
    const sameTag = Array.from(parent.children).filter((c) => c.tagName === el.tagName);
    return sameTag.indexOf(el);
  }

  function uniqueWithin(root, selector) {
    try {
      return root.querySelectorAll(selector).length === 1;
    } catch {
      return false;
    }
  }

  function cssEscape(s) {
    // window.CSS.escape is widely supported but we vendor the
    // fallback for the few WebKit builds that lack it.
    if (window.CSS && typeof window.CSS.escape === "function") {
      return window.CSS.escape(s);
    }
    return String(s).replace(/[^\w-]/g, (ch) => `\\${ch}`);
  }

  // ── Overlay UI ───────────────────────────────────────────────
  const banner = document.createElement("div");
  banner.style.cssText = `
    position: fixed; top: 0; left: 0; right: 0;
    z-index: 2147483647;
    background: #1c1717; color: #fff8e0;
    font: 13px/1.4 'Silkscreen', monospace;
    padding: 8px 12px;
    border-bottom: 2px solid #ff6680;
  `;
  banner.textContent = "Springtale picker: hover an element, click to pick. Esc to cancel.";
  if (
    ALLOWLIST.length > 0 &&
    !ALLOWLIST.some((h) => location.hostname === h || location.hostname.endsWith(`.${h}`))
  ) {
    banner.style.background = "#3a1c1c";
    banner.textContent += ` ⚠️ host ${location.hostname} not in recipe allow-list`;
  }
  document.body.appendChild(banner);

  let hovered = null;
  let savedOutline = null;
  function setHovered(el) {
    if (hovered && hovered !== el) {
      hovered.style.outline = savedOutline;
    }
    hovered = el;
    if (el) {
      savedOutline = el.style.outline;
      el.style.outline = HIGHLIGHT_OUTLINE;
    }
  }

  document.addEventListener(
    "mouseover",
    (ev) => {
      if (banner.contains(ev.target)) return;
      setHovered(ev.target);
    },
    true,
  );

  document.addEventListener(
    "click",
    (ev) => {
      if (banner.contains(ev.target)) return;
      ev.preventDefault();
      ev.stopImmediatePropagation();
      const selector = finder(ev.target);
      const tag_name = ev.target.tagName.toLowerCase();
      emit({ selector, tag_name });
    },
    true,
  );

  document.addEventListener("keydown", (ev) => {
    if (ev.key === "Escape") {
      emit(null);
    }
  });

  function emit(payload) {
    setHovered(null);
    banner.remove();
    window.__SPRINGTALE_PICKER_ACTIVE__ = false;
    // Tauri 2 IPC: __TAURI_INTERNALS__.invoke is the v2 path.
    // We use the event API via the global injected by Tauri's
    // webview adapter.
    if (window.__TAURI__?.event) {
      window.__TAURI__.event.emit("selector-picked", payload || {});
    } else if (window.ipc && typeof window.ipc.postMessage === "function") {
      // Fallback to a postMessage envelope; the Rust side listens
      // for a string-typed message in this format.
      window.ipc.postMessage(JSON.stringify({ event: "selector-picked", payload: payload || {} }));
    }
  }
})();
