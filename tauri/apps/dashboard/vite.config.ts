/// <reference types="node" />
import { createHash } from "node:crypto";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig, type Plugin } from "vite";
import solidPlugin from "vite-plugin-solid";

/**
 * Sub-Resource Integrity plugin — inline.
 *
 * OWASP A05 + A08 (Software Integrity Failures). After Vite/Rollup
 * emits the bundle, walk every `<script src="…">` and
 * `<link rel="stylesheet" href="…">` reference in the index HTML, hash
 * the matching emitted asset with SHA-384, and inject
 * `integrity="sha384-…" crossorigin="anonymous"`.
 *
 * Implemented inline rather than via a community npm plugin so we add
 * no new supply-chain dependency for ~30 lines of crypto + tag
 * rewriting — `node:crypto` is a Node builtin. Single-maintainer SRI
 * plugins (vite-plugin-sri, vite-plugin-sri3) would be an inferior
 * trade for this project's threat model.
 *
 * SHA-384 matches what the `tauri-apps/tauri` project recommends for
 * Tauri 2 webview SRI and exceeds the legacy SHA-256 minimum from the
 * SRI W3C recommendation.
 */
function springtaleSri(): Plugin {
  return {
    name: "springtale-sri",
    enforce: "post",
    apply: "build",
    transformIndexHtml: {
      order: "post",
      handler(html, ctx) {
        const bundle = ctx?.bundle;
        if (!bundle) return html;

        const integrityFor = (refPath: string): string | null => {
          const key = refPath.replace(/^\/+/, "");
          const asset = bundle[key];
          if (!asset) return null;
          const body =
            asset.type === "chunk"
              ? asset.code
              : typeof asset.source === "string"
                ? asset.source
                : Buffer.from(asset.source);
          const digest = createHash("sha384")
            .update(typeof body === "string" ? Buffer.from(body) : body)
            .digest("base64");
          return `sha384-${digest}`;
        };

        const rewrite = (
          tag: "script" | "link",
          srcAttr: "src" | "href",
          input: string,
        ): string => {
          const re = new RegExp(`<${tag}\\b([^>]*?)\\s${srcAttr}=["']([^"']+)["']([^>]*)>`, "g");
          return input.replace(re, (full, pre: string, srcVal: string, post: string) => {
            // Skip absolute / external references — SRI cannot hash
            // content we don't control. Same-origin relative paths
            // and `/`-rooted paths point at bundle assets and DO get
            // hashed.
            if (/^(?:https?:)?\/\//.test(srcVal) || srcVal.startsWith("data:")) {
              return full;
            }
            const integrity = integrityFor(srcVal);
            if (!integrity) return full;
            // Avoid duplicating attributes if the dev added them.
            if (/\bintegrity=/.test(pre) || /\bintegrity=/.test(post)) return full;
            // Vite emits a boolean `crossorigin` attribute on module
            // scripts / module-preload links by default. Skip adding
            // ours when one is already present — duplicate attributes
            // are valid HTML but unsightly and the SRI W3C
            // recommends a single explicit `crossorigin="anonymous"`.
            const hasCrossorigin = /\bcrossorigin\b/.test(pre) || /\bcrossorigin\b/.test(post);
            const crossoriginAttr = hasCrossorigin ? "" : ' crossorigin="anonymous"';
            return `<${tag}${pre} ${srcAttr}="${srcVal}"${post} integrity="${integrity}"${crossoriginAttr}>`;
          });
        };

        let out = html;
        out = rewrite("script", "src", out);
        out = rewrite("link", "href", out);
        return out;
      },
    },
  };
}

export default defineConfig({
  plugins: [tailwindcss(), solidPlugin(), springtaleSri()],
  server: {
    port: 5173,
  },
  build: {
    target: "esnext",
  },
});
