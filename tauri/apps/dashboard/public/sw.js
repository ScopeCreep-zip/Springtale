/**
 * Springtale dashboard service worker (W5 — PWA installability).
 *
 * Deliberately minimal and privacy-preserving: it caches only the static
 * app shell so the PWA is installable and the UI loads offline. It NEVER
 * caches API responses — `/chat`, `/events`, `/approvals`, and every other
 * data route always hit the live loopback daemon. No user data is persisted
 * in the cache, consistent with the local-first / zero-retention posture.
 */

const SHELL_CACHE = "springtale-shell-v1";
const SHELL_ASSETS = ["/", "/index.html", "/manifest.webmanifest", "/icon-192.png", "/icon-512.png"];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches.open(SHELL_CACHE).then((cache) => cache.addAll(SHELL_ASSETS)).then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) =>
        Promise.all(keys.filter((k) => k !== SHELL_CACHE).map((k) => caches.delete(k))),
      )
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (event) => {
  const url = new URL(event.request.url);

  // Never touch the cache for API / SSE traffic — always go to the live
  // daemon. Heuristic: same-origin GET for a known static asset path is
  // cacheable; everything else (POST, /chat, /events, /approvals, …) is not.
  const isStaticGet =
    event.request.method === "GET" &&
    url.origin === self.location.origin &&
    !url.pathname.includes("/stream") &&
    (url.pathname === "/" ||
      url.pathname.startsWith("/assets/") ||
      url.pathname.endsWith(".html") ||
      url.pathname.endsWith(".png") ||
      url.pathname.endsWith(".css") ||
      url.pathname.endsWith(".js") ||
      url.pathname.endsWith(".webmanifest"));

  if (!isStaticGet) {
    return; // default network behaviour
  }

  event.respondWith(
    caches.match(event.request).then((cached) => cached || fetch(event.request)),
  );
});
