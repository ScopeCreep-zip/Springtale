---
paths:
  - "tauri/**/*.{ts,tsx,css}"
  - "tauri/**/*.html"
---

# SolidJS + Tauri Frontend Conventions

## Framework
- SolidJS 1.9+ (fine-grained reactivity, signals, stores)
- Tailwind 4 (utility-first, `@theme` for custom properties)
- Vite 6 (build tool, HMR)
- Tauri 2 (desktop shell, IPC via `invoke()`)

## Architecture
- `packages/ui/` — shared component library (no app-specific logic)
- `packages/types/` — shared TypeScript types
- `apps/desktop/` — Tauri desktop app
- `apps/dashboard/` — web SPA served by springtaled

## Colony visual system
- All UI uses the colony pixel-art theme from `colony.css` and `sprites.css`
- Colors from soil palette (not Tailwind defaults)
- Silkscreen pixel font
- CSS classes over inline styles. Only dynamic `width`/`left`/`top` as inline.
- Sprite classes defined in `@layer components` in CSS, not inline box-shadow.

## Data flow
- `DataProvider` interface (platform-agnostic) — defined in `dashboard/types.ts`
- Desktop provider wraps Tauri IPC (`invoke()`)
- Web provider wraps HTTP fetch + SSE
- `createDashboardState(provider)` factory → `useDashboard()` hook
- NO raw `invoke()` calls in components. Always go through provider.

## Component rules
- One component per file, named exports only
- No `innerHTML` or `dangerouslySetInnerHTML` — SolidJS auto-escapes
- No `@apply` except in colony CSS files
- Tailwind utility classes in `class=` attribute
- `@source` directive in index.css to scan shared UI package

## No fake data
- Every visual signal maps to real backend state
- No `Math.random()` for data values
- Seeded positions (deterministic from hash) are acceptable for layout
- Decorative elements (litter dots, mushrooms, ground texture) are acceptable
