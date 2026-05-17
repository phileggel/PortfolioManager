# Architecture

VaultCompass is a single-user Tauri 2 desktop app. React 19 + TypeScript on the frontend, Rust + SQLite on the backend, Specta-generated bindings for IPC, DDD for backend layering.

> **For Claude Code**: this file is a **router**, not a code catalog. Per-module file lists, command signatures, and entity field shapes are NOT documented here — they drift the moment code changes and are one `ls` or `grep` away. For implementation questions, start with the doc pointers below and read the actual source.

---

## Stack

| Layer         | Tech                                                                                                              |
| ------------- | ----------------------------------------------------------------------------------------------------------------- |
| Desktop shell | Tauri 2 (single executable)                                                                                       |
| Frontend      | React 19 + TypeScript, Zustand store, react-i18next                                                               |
| Backend       | Rust, async via tokio, SQLite via sqlx (compile-time query checking)                                              |
| IPC           | Specta-generated bindings — `src/bindings.ts` (auto-generated, do not edit; regenerate via `just generate-types`) |

---

## Where to look

| You need                                                                | Read                                                                                                                 |
| ----------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| Backend implementation patterns (row mapping, orchestrator shape, etc.) | [`docs/backend-patterns.md`](docs/backend-patterns.md)                                                               |
| Generic DDD rules (kit-managed)                                         | [`docs/backend-rules.md`](docs/backend-rules.md), [`docs/ddd-reference.md`](docs/ddd-reference.md)                   |
| Intentional divergences from textbook DDD                               | [`docs/ddd-divergences.md`](docs/ddd-divergences.md)                                                                 |
| Error model (per-BC enum + per-use-case composite)                      | [`docs/error-model.md`](docs/error-model.md)                                                                         |
| Frontend rules + visual proof workflow                                  | [`docs/frontend-rules.md`](docs/frontend-rules.md), [`docs/frontend-visual-proof.md`](docs/frontend-visual-proof.md) |
| i18n + a11y rules                                                       | [`docs/i18n-rules.md`](docs/i18n-rules.md)                                                                           |
| E2E + test conventions                                                  | [`docs/e2e-rules.md`](docs/e2e-rules.md), [`docs/test_convention.md`](docs/test_convention.md)                       |
| Business rules per feature                                              | [`docs/spec/{feature}.md`](docs/spec/)                                                                               |
| Architectural decisions                                                 | [`docs/adr/{NNN}-*.md`](docs/adr/)                                                                                   |
| Domain terms (canonical)                                                | [`docs/ubiquitous-language.md`](docs/ubiquitous-language.md)                                                         |
| Tech debt                                                               | [`docs/techdebt.md`](docs/techdebt.md)                                                                               |
| Open items                                                              | [`docs/todo.md`](docs/todo.md)                                                                                       |
| Roadmap                                                                 | [`docs/roadmap.md`](docs/roadmap.md)                                                                                 |
| Design system tokens                                                    | [`docs/design-system.md`](docs/design-system.md), [`docs/theme.md`](docs/theme.md)                                   |
| Kit-managed tools                                                       | [`.claude/kit-tools.md`](.claude/kit-tools.md)                                                                       |

For "what does use case X do?" → read `src-tauri/src/use_cases/{name}/mod.rs` (module doc) and its spec at `docs/spec/`.
For "what does feature X do?" → read `src/features/{name}/` and its spec.

---

## Backend layout (`src-tauri/src/`)

```
context/{bc}/         bounded contexts — strict module isolation, no cross-BC imports
  domain/             entities + repository traits (persistence-ignorant)
  repository/         SQLite repository implementations (FromRow + impl From<Row> for Domain)
  service.rs          application service (thin: load → mutate → save → publish event)
  api.rs              BC-owned Tauri commands
  error.rs            flat {BC}Error enum (per docs/error-model.md)
  mod.rs              public API surface (use only this from outside)

use_cases/{name}/     cross-BC orchestrators (read or write paths spanning multiple BCs)
  orchestrator.rs     single struct, one method per Tauri command
  error.rs            per-use-case composite(s) + use-case-specific flat failures
  api.rs              use-case-owned Tauri commands
  mod.rs

core/                 shared infra
  db.rs               sqlite pool + migrations
  event_bus/          SideEffectEventBus + Event enum (cross-BC pub/sub)
  logger.rs           tracing setup, FRONTEND / BACKEND targets
  specta_builder.rs   THE Tauri command registry (every command registered here, nowhere else)
  cash.rs             system cash asset id helpers (shared without crossing a BC boundary)

lib.rs                composition root — wires services, use cases, dispatchers; calls app_handle.manage()
```

**Hard rules:**

- Tauri commands are registered ONLY in `core/specta_builder.rs`. Adding a `#[tauri::command]` without registering it there silently breaks IPC.
- Bounded contexts cannot import from each other. Cross-BC logic lives in `use_cases/`.
- Repositories return `Result<T, anyhow::Error>`. Services translate to typed `{BC}Error`. Use cases compose via `#[from]`. See `docs/error-model.md`.
- See [`docs/backend-patterns.md`](docs/backend-patterns.md) for the row-mapping recipe and orchestrator shape.

---

## Event bus

Backend publishes events on every state change. Frontend listens via a single `events.event.listen()` subscription in `src/lib/store.ts:init()` and dispatches to the right fetcher.

| Event                | Published by                                                   | Frontend re-fetches                          |
| -------------------- | -------------------------------------------------------------- | -------------------------------------------- |
| `AssetUpdated`       | `context/asset/` writes                                        | `assets`                                     |
| `CategoryUpdated`    | `context/asset/` category writes                               | `categories`                                 |
| `AssetPriceUpdated`  | `context/asset/` price writes + `use_cases/asset_price_fetch/` | `account_details` (per-page)                 |
| `AccountUpdated`     | `context/account/` account writes                              | `accounts`                                   |
| `TransactionUpdated` | `context/account/` holding / transaction writes                | `account_details`, `transactions` (per-page) |

Adding a new event: declare the variant in `core/event_bus/event.rs`, publish from the service after persistence (`bus.publish(Event::Foo)`), subscribe in the relevant feature hook.

---

## Frontend layout (`src/`)

```
bindings.ts          AUTO-GENERATED Tauri bindings — do not edit
features/{name}/     feature-first layout (see "Feature layout convention" below)
infra/               cross-feature plumbing (storage helpers, logger wrapper, fuzzy search)
ui/                  M3 design primitives (Button, Field, Modal, Layout)
shared/              cross-feature utilities (Result helpers, presenter primitives)
i18n/                react-i18next config + locales (fr default, en fallback)
lib/                 legacy bucket — migration to infra/ + shared/ in progress (see docs/techdebt.md)
```

### Data flow

```
Component
  └─ Hook (state, useMemo, callbacks)
       └─ Gateway (commands.* — positional args, matches bindings.ts exactly)
            └─ Tauri IPC
                 └─ Rust api.rs handler (Result<T, {Command}Error>)
                      └─ Use case / Service
                           └─ Repository
                                └─ SQLite

Backend publishes Event
  └─ src/lib/store.ts:init() listener
       └─ store re-fetches the affected slice → subscribed components re-render
```

### Feature layout convention (gold)

All new features MUST follow this. Reference: `features/assets/`.

```
features/{domain}/
├── gateway.ts                  # ONLY file that calls commands.* for this feature
├── {sub_feature}/
│   ├── {SubFeature}.tsx        # Component
│   ├── use{SubFeature}.ts      # Colocated hook
│   └── use{SubFeature}.test.ts # Colocated test
├── shared/
│   ├── presenter.ts            # Domain → UI transformations
│   ├── validate{X}.ts          # Pure validation
│   └── constants.ts
└── index.ts                    # Public re-exports
```

Hard rules:

- `gateway.ts` at the feature root — never `api/` wrappers, never inline `commands.*` calls in components or hooks.
- Sub-features are directories grouped by **concern**, not by layer (no `components/`, `hooks/`).
- Hooks colocated next to their component inside the sub-feature folder.
- `presenter.ts` is pure — no `commands.*`, no `useEffect`.
- See [`docs/frontend-rules.md`](docs/frontend-rules.md) for the full rule set (F1–F28).

---

## Decisions to know about

These are the load-bearing architectural choices that aren't obvious from reading code:

- **Single SQLite database** with foreign keys spanning BCs. Logical BC isolation only (Rust module level). See [`docs/ddd-divergences.md`](docs/ddd-divergences.md) #7.
- **Domain types serialize directly to the FE** via Specta (no DTO layer). See [`docs/ddd-divergences.md`](docs/ddd-divergences.md) #3.
- **IDs are `String`, not value-object wrappers.** See [`docs/ddd-divergences.md`](docs/ddd-divergences.md) #1.
- **Repository folder is still named `repository/`** (gold standard is `infrastructure/`). Bit-by-bit migration tracked in [`docs/techdebt.md`](docs/techdebt.md).
- **`anyhow::Error` in repo trait error type** is intentional. See [`docs/ddd-divergences.md`](docs/ddd-divergences.md) #10.
- **Update lifecycle state (`UpdateState`)** is managed before the DB is ready so the frontend can show a migration-failure screen.

---

## Maintenance

This file describes what doesn't change often. Per-feature / per-use-case / per-migration details belong in source code, spec docs, or the kit pattern docs — not here.

Update this file when:

- A new top-level module appears under `src-tauri/src/` or `src/`
- A new event is added to the event bus
- An architectural decision changes (DB swap, layering reshape, IPC rework)
- A new specialized doc joins `docs/` and is worth routing to

Do NOT update this file when adding a use case, a BC method, a Tauri command, or a feature module — those live in source code and their own specs.
