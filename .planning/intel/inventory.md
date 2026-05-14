# Inventory

Running map of this project's surface. Update when you add/remove/rename major pieces.

## Frontend (`src/`)

- `main.tsx` — React entry
- `App.tsx` — root component
- `index.css` — Tailwind 4 + SYVR Admin Dark theme tokens
- `lib/utils.ts` — shadcn `cn()` helper
- `components/ui/` — shadcn components (installed via `pnpm dlx shadcn@latest add ...`)

## Backend (`src-tauri/`)

- `src/main.rs` — Rust entry (thin; delegates to `lib.rs`)
- `src/lib.rs` — Tauri builder + `#[tauri::command]` handlers
- `tauri.conf.json` — window config, bundle targets, CSP
- `capabilities/default.json` — permission grants (extend when adding plugins)

## Build artifacts (gitignored)

- `dist/` — Vite frontend build
- `src-tauri/target/` — Rust cargo output
- `src-tauri/gen/` — Tauri codegen (plugin schemas)

## External services

_None wired by default._ Add sections here as you integrate Convex / Postgres / Workers / etc.
