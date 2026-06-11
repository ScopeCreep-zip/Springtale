/// <reference types="node" />
import { createHash } from "node:crypto";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig, type Plugin } from "vite";
import solidPlugin from "vite-plugin-solid";

// Tauri expects a fixed port for the dev server
const TAURI_DEV_HOST = process.env.TAURI_DEV_HOST;

/**
 * Sub-Resource Integrity plugin — inline.
 *
 * Twin of the dashboard's SRI plugin (`apps/dashboard/vite.config.ts`).
 * Duplicated rather than packaged because each app's `vite.config.ts`
 * stands alone and the plugin is small enough that drift is easy to
 * audit by diff. SHA-384 matches the Tauri 2 webview SRI guidance and
 * defends against on-disk tampering of bundled assets in the device-
 * seizure threat model.
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
            if (/^(?:https?:)?\/\//.test(srcVal) || srcVal.startsWith("data:")) {
              return full;
            }
            const integrity = integrityFor(srcVal);
            if (!integrity) return full;
            if (/\bintegrity=/.test(pre) || /\bintegrity=/.test(post)) return full;
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
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: TAURI_DEV_HOST || false,
  },
  build: {
    target: "esnext",
  },
});
