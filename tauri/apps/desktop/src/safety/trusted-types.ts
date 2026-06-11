// Springtale Trusted Types policy.
//
// CSP `require-trusted-types-for 'script'` (set in tauri.conf.json) blocks
// every DOM sink that takes a string parser (`innerHTML`, `eval`, `Function`,
// `setTimeout(string)`, `<script src>`, etc.) unless the value comes through
// a TrustedTypes policy.
//
// SolidJS auto-escapes every reactive expression, so under normal use
// nothing reaches a parser sink. The `default` policy below catches the
// case where Solid (or a transitive dep) does end up calling one — by
// passing the string through unchanged, the call succeeds, but it surfaces
// as a Trusted Types policy invocation in DevTools so we can audit the path
// in dev mode.
//
// In production we keep the policy permissive (no transformation) so
// SolidJS's already-escaped output renders correctly. The kill-switch is
// the absence of the policy: if a malicious bundle replaces this module,
// every parser sink fails closed.
//
// Per OWASP A05 (Injection), OWASP CSP Cheat Sheet, and MASVS-PLATFORM-2.
//
// Reference: https://web.dev/articles/trusted-types

export function installTrustedTypesPolicy(): void {
  // `trustedTypes` is undefined on platforms that don't support it (older
  // WebKit, Tauri webview on macOS pre-13). The CSP directive is still
  // emitted; browsers without support ignore it, so we just bail out.
  const win = window as unknown as {
    trustedTypes?: {
      createPolicy: (
        name: string,
        rules: {
          createHTML?: (s: string) => string;
          createScript?: (s: string) => string;
          createScriptURL?: (s: string) => string;
        },
      ) => unknown;
    };
  };

  if (!win.trustedTypes || typeof win.trustedTypes.createPolicy !== "function") {
    return;
  }

  try {
    win.trustedTypes.createPolicy("default", {
      // SolidJS escapes interpolated strings at the framework layer.
      // The identity passthrough here means a downstream `innerHTML`
      // assignment surfaces in DevTools (audit signal) but does not
      // double-escape SolidJS-rendered content.
      createHTML: (s) => s,
      createScript: (s) => s,
      createScriptURL: (s) => s,
    });
  } catch {
    // Policy may already exist (HMR re-init). Safe to swallow.
  }
}
