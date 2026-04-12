# connector-browser

Headless Chromium browser automation. Navigate pages, fill forms, click elements, extract text, and capture screenshots. Domains are restricted at manifest load — the connector cannot reach hosts that aren't in the allow-list.

```
   Rule engine                    connector-browser                   Chromium
        │                                 │                             │
        │ execute(navigate, {url})        │                             │
        ├────────────────────────────────►│                             │
        │                                 │ host ∈ allowed_domains?     │
        │                                 │                             │
        │                                 │ CDP: Page.navigate          │
        │                                 ├────────────────────────────►│
        │                                 │                             │
        │                                 │ page_loaded event           │
        │                                 │ ◄───────────────────────────┤
        │  emit page_loaded trigger       │                             │
        │  ◄──────────────────────────────┤                             │
        │                                 │                             │
        │ execute(click, {selector})      │                             │
        ├────────────────────────────────►│ CDP: Runtime.evaluate       │
        │                                 ├────────────────────────────►│
        │                                 │                             │
        │ execute(screenshot)             │ CDP: Page.captureScreenshot │
        ├────────────────────────────────►├────────────────────────────►│
        │  PNG bytes                      │                             │
        │  ◄──────────────────────────────┤ ◄───────────────────────────┤

  Profile is fresh per invocation and deleted on shutdown. No cookies
  or localStorage survive across runs.
```

*Fig. 1. Browser data flow over Chrome DevTools Protocol. Every navigation is gated on `allowed_domains`; the browser profile is ephemeral.*

## 1. Configuration

**TABLE I. CONFIG FIELDS**

| Field | Type | Default | Description |
|---|---|---|---|
| `allowed_domains` | `Vec<String>` | (required) | Hostnames the browser may navigate to. Each becomes a `NetworkOutbound` capability. |
| `chrome_path` | `Option<String>` | auto-detect | Path to Chrome/Chromium binary. If unset, probes standard locations. |
| `disable_telemetry` | `bool` | `true` | Launches Chromium with telemetry flags disabled. |
| `message_jitter_secs` | `u64` | `0` | Random delay 0..N seconds on sends, for timing obfuscation. |

## 2. Authentication

None. Access is gated by the `allowed_domains` allow-list.

## 3. Triggers

**TABLE II. TRIGGERS**

| Name | Description | Payload fields |
|---|---|---|
| `page_loaded` | Fires when a navigated page finishes loading | `url`, `title`, `status` |
| `element_found` | Fires when a CSS selector matches on the current page | `selector`, `found` (bool), `text` |

## 4. Actions

**TABLE III. ACTIONS**

| Name | Input fields | Output fields |
|---|---|---|
| `navigate` | `url: String` | `url`, `status` |
| `fill_form` | `selectors: Map<String, String>` | — |
| `click` | `selector: String` | — |
| `screenshot` | optional `region: Rect` | `bytes` (PNG) |
| `extract_text` | `selector: String` | `text` |

## 5. Capabilities Required

| Capability | Parameter |
|---|---|
| `NetworkOutbound` | one entry per host in `allowed_domains` |

## 6. Example Rule

```toml
[rule]
name = "daily-price-check"

[trigger]
type = "Cron"
expression = "0 9 * * *"

[[actions]]
type = "RunConnector"
connector = "connector-browser"
action = "navigate"

[actions.params]
url = "https://example.com/product/42"

[[actions]]
type = "RunConnector"
connector = "connector-browser"
action = "extract_text"

[actions.params]
selector = ".price"
```

## 7. Security & Privacy

- **No persistent state.** The browser profile is created fresh per invocation and deleted on shutdown. No cookies, no localStorage, no cached credentials survive.
- **Inherent risk: JavaScript execution on untrusted pages.** The browser still runs JS from whatever you navigate to. Don't navigate to hostile domains and expect the sandbox alone to protect you.
- **IP exposure.** Target domains see your IP. Use a VPN if that matters.
- **Capability enforcement at load time.** Navigation to any host outside `allowed_domains` is rejected before the browser even touches the network.
