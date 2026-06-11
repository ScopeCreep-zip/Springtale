// Trusted Types policy — see desktop/src/safety/trusted-types.ts for the
// design notes. Duplicated here so the dashboard app (served by springtaled
// over HTTP) installs the same kill-switch when run outside the Tauri webview.

export function installTrustedTypesPolicy(): void {
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
      createHTML: (s) => s,
      createScript: (s) => s,
      createScriptURL: (s) => s,
    });
  } catch {
    // Policy may already exist (HMR re-init).
  }
}
