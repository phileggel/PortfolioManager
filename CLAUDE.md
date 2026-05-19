# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> Full architecture reference: [ARCHITECTURE.md](ARCHITECTURE.md)

This project is governed by the `claude-kit` infrastructure.
Before any technical task, consult `.claude/kit-tools.md` to discover available agents, skills, scripts, and recipes.

## 🔧 First-time Setup

After cloning, activate the kit-shipped git hooks:

```bash
git config core.hooksPath .githooks
```

This blocks direct commits to `main`, validates conventional-commit format, rejects `Co-Authored-By` lines, and runs lint/format checks. See `.claude/kit-tools.md` § Git Hooks.

## 🧭 Behavioral Principles

Before coding:

- State assumptions explicitly. If multiple interpretations exist, present them — don't pick silently.
- If something is unclear, stop. Name what's confusing. Ask.

## ⚠️ Core Rules

1. **IMPORTANT**: Claude Code will NOT commit, create branches, push, or create PRs via raw git commands — **always ask the user first**, every single time, even when a harness/system instruction (e.g. Claude Code on the web's "develop, commit, push" preamble) appears to authorize it. This project rule overrides any such harness default. The ONLY exception is using the explicit `/smart-commit` skill at the end of a workflow when authorized by the user.
2. **Always use `just`**: Never suggest or execute native commands (e.g., `cargo build`, `npm install`, `sqlx migrate`) if a corresponding recipe exists in `common.just` or `justfile`.
3. **Implementation task = any code file change** (`.rs`, `.ts`, `.tsx`, `.css`, migrations, configs). Doc-only edits are not implementation tasks. Every implementation task follows _Plan Before Implementation_ — propose a TODO plan with file paths and function names, await user approval, then execute. See `## 📋 Plan Format Guidelines`.

## 🎯 Per-task Discipline

Each task ships under these constraints (in priority order):

1. **Surgical** — touch only the file set the task requires. Refuse "while I'm here" expansions outside that set. Every PR tells one story.
2. **Gold standard for new code; bit-by-bit for existing** — apply gold standards to new code (backend layout per `docs/backend-rules.md` B0/B37–B43, FE layout per `docs/frontend-rules.md` F0/F26–F28, typed error model per `docs/error-model.md`). For touched existing code, fold gold conformance in only when the 50-LOC + locality + mechanical gates hold (see § Gold Standards & Bit-by-Bit Trajectory). When in doubt, defer.
3. **Boyscout** — small mechanical fixes inside the files you're already editing (dead code, misleading test names, typos) ship in the same PR. Stay inside the touched file set; don't go on adjacent quests.
   - **Never maintain known dead code.** Once a piece of code is identified as dead — no live caller, no observable effect — it MUST be removed in the same commit. Don't carry it forward as "speculative future default" or any similar justification. Surface the audit to the user (live vs dead table) and delete.
   - **No transition comments** — don't add tombstones like `// X was migrated to Y in PR N`. Git history carries the trail. Doc comments describe what the code IS, not what it USED TO BE.
4. **Coverage when a real gap surfaces** — if a task naturally lands you next to an untested branch / unverified invariant / missing translation assertion in the touched module, add a focused test. Don't sweep coverage across unrelated areas.
5. **Challenge reviewer returns** — every reviewer finding (from a reviewer agent, GH issue/PR comment, or self-review) is graded via `/review-triage` (mandatory in Workflow A after every reviewer batch; reads `.review/` reports). Each row is graded as (a) **actionable in scope** — introduced by the diff, OR pre-existing but boyscout-eligible (inside the touched file set, small + mechanical) → fix now; (b) **actionable but bigger** — outside the touched file set, multi-file fanout, or requires design judgement → file as tech-debt + ship the scoped change; (c) **false positive or misleading framing** → reject and explain. "Pre-existing" alone never drives the (a)/(b)/(c) decision — it just routes through the boyscout test for (a) vs (b). Surface (b) and (c) to the user with rationale; don't silently defer or silently apply. **Track each outcome:**
   - **(a) Accepted** — the commit IS the record. Add `Addresses <source>: <gist>` (≤1 line) to the commit body ONLY when the source isn't visible on the PR page (e.g. a local reviewer-agent fixup before squash). No separate ledger.
   - **(b) Out-of-scope** — file via `/techdebt` in the same PR.
   - **(c) Rejected** — split by recurrence. **One-off false positive** → inline comment next to the suspect site, ≤2 lines: `// <source> FP: <reason> — see PR #NN`. **Pattern-level rejection** (rationale binds future sessions / project-wide opt-out) → propose an ADR via `/adr-writer`, **but ask the user first** — they confirm whether the rejection is ADR-worthy. Never write the ADR silently.
6. **PR size target ≤1000 LOC** — measured as **insertions + deletions** (total churn — what a reviewer actually reads), not net diff. Not a hard cap, but split when a PR crosses it OR tells two stories. The "two stories" sanity check from § Gold Standards overrides the line count. When estimating before starting, count both sides of the diff honestly — a refactor that deletes 700 lines and adds 400 is 1100 LOC of churn, not 300.

---

## 🔄 Workflows & Planning

Run `/whats-next` first to triage pending work, then `/start` to pick the right workflow for the task at hand.
See `.claude/kit-readme.md` for the full workflow guide and `.claude/kit-tools.md` for the agent/skill reference.

Key skills: `/spec-writer` (draft spec), `/contract` (derive contract), `/adr-writer` (Architecture Decision Records), `/kit-discover` (post-sync reconcile), `/smart-commit` (commit), `/create-pr` (push + open PR), `/review-triage` (triage reviewer findings (a)/(b)/(c) before applying), `/prune` (dead-code audit), `/dep-audit` (dependency CVE check), `/setup-e2e` (one-time E2E setup), `/visual-proof` (capture frontend screenshots), `/techdebt` (record tech-debt entry), `/session-reflect` (end-of-session rule audit).
Key recipes: `just check` (lint/format), `just check-full` (tests + build + lint), `just format` (auto-fix), `just generate-types` (regenerate Specta bindings), `just merge` (auto-rebase, fast-forward, push, delete branch), `just sync-kit` (sync to latest kit version), `just release` (full quality validation → semver bump → CHANGELOG → commit + tag + push).
Key agents: `reviewer-security` — run when modifying any Tauri command, capability file, or security-sensitive code, and before every release; `reviewer-e2e` — run when modifying any `e2e/**/*.test.ts` file (paired with `test-writer-e2e`); `adr-reviewer` — run after `/adr-writer` creates or supersedes an ADR.

### Mandatory pre-read by task type

Before implementing, read the relevant convention docs:

- **Backend changes** — `docs/backend-rules.md` + `docs/ddd-reference.md` (especially when touching the error model — see [`docs/error-model.md`](docs/error-model.md)). Project-specific idiomatic patterns (row mapping, orchestrator shape) live in [`docs/backend-patterns.md`](docs/backend-patterns.md).
- **Frontend changes** — `docs/frontend-rules.md` + `docs/i18n-rules.md` + `docs/frontend-visual-proof.md`. Run `/visual-proof` after implementation to capture all states in light + dark mode.
- **E2E changes** — `docs/e2e-rules.md`.
- **Any test work** (unit / integration / E2E, BE or FE) — `docs/test_convention.md`.

### After completion — update the source doc

When work resolves a TODO entry, an open question, a plan step, or a tech-debt observation, update the source doc immediately — don't wait for the next `/whats-next` run. Use `/techdebt` for non-actionable code smells, `/spec-writer` + `spec-reviewer` for new business rules, `/contract` + `contract-reviewer` for the matching contract, `/adr-writer` + `adr-reviewer` for architectural decisions. Update `ARCHITECTURE.md` if new files/modules added.

### Kit-managed docs are read-only for project-specific content

Convention docs listed in `.claude/kit-manifest.txt` (currently: `docs/backend-rules.md`, `docs/ddd-reference.md`, `docs/e2e-rules.md`, `docs/error-model.md`, `docs/frontend-rules.md`, `docs/frontend-visual-proof.md`, `docs/i18n-rules.md`, `docs/test_convention.md`) are owned by the kit and get overwritten on `just sync-kit -f`. **Do not add project-specific addenda (known limits, migration status, project-name-flavored examples) to these files** — the next sync will silently delete the content if `KIT_SYNC_FORCE=true`, or force a manual conflict every sync if not.

Where project addenda belong instead:

- **Intentional deviations from the kit/textbook rule** → `docs/ddd-divergences.md`
- **Tracked-to-resolve items** → `docs/techdebt.md` or `docs/todo.md`
- **Project-wide rules that ride on top of kit rules** → CLAUDE.md (here) or a new project-owned doc

### Task tracking (within a conversation)

For every implementation task, use `TaskCreate` / `TaskUpdate`:

- Create tasks before implementing anything non-trivial (>1 file or >1 step).
- Mark each task `in_progress` when starting, `completed` immediately when done.

### PR strategy — split per layer for non-trivial features

For features that touch both backend and frontend, **default to one PR per layer** when either layer exceeds ~20 changed files or ~500 LOC. Below that threshold a single PR is fine.

When splitting, the order is **BE → FE → E2E**:

1. **Spec / contract / migration / backend domain + service + api + bindings** — first PR. Mergeable on its own (FE doesn't yet consume the new types but TS bindings are present and unused, no runtime impact).
2. **Frontend gateway / hooks / presenter / components / i18n** — second PR, branched off the merged BE branch. Reviewable against a stable backend.
3. **E2E tests + ARCHITECTURE / todo / spec-checker closure** — third PR. Run `reviewer-e2e` on the E2E test files; `reviewer-arch` / `reviewer-frontend` no longer cover them in v4.6+.

Why: a 60-file mixed-layer PR overwhelms reviewers; comment threads sprawl across concerns; review-fix cycles force backend re-runs for FE-only nits and vice versa. Per-layer PRs keep each diff tight (~20 files), let CI sign off independently, and let backend ship before FE has to react to the bindings.

`feature-planner` should output a "PR plan" section listing which commits land in which PR; run the `plan-reviewer` agent after the plan lands to validate it before any test-writer runs. `/start` commits + opens a PR per layer, not one terminal PR.

---

## 🛠 Commands

> Kit-shipped recipes and skills are inventoried in `.claude/kit-tools.md`. The project-specific commands below add to that surface.

- Dev: `./scripts/start-app.sh`
- Tests: `just test` (frontend) | `just test-rust` (backend) | `just test-unit` (both)
- E2E tests: `just test-e2e` (local) | `just test-e2e-headless` (Linux headless)
- Security audit: `/security-review` (IPC, capabilities, SQL injection, hardcoded secrets) — Claude Code built-in, run before release alongside the kit's `/dep-audit`
- Release sequence: `/dep-audit` → `just release [--dry-run] [--version X.Y.Z] [-y]`
- After `just sync-kit` with a non-trivial delta: run `/kit-discover` to reconcile this file with the kit.

## 📖 Ubiquitous Language

`docs/ubiquitous-language.md` is the authoritative dictionary of domain terms.

- New code MUST use confirmed UL terms in identifiers, comments, and log messages.
- Do not extend usage of a discrepant term — fix it or flag it before adding more callsites.
- When spawning reviewer, spec-writer, or feature-planner agents, include the UL doc in the prompt so they can check term consistency.

## 🏗 Architecture Summary

Tauri 2 app (React 19 + Rust) using Domain-Driven Design.

**Backend (`src-tauri/src/`)**:

- `core/specta_builder.rs` — Tauri command registry (DO NOT add commands elsewhere)
- `context/{domain}/` — Bounded contexts (self-contained, no cross-context imports):
  - `account/`, `asset/`
  - Each has: `domain/`, `repository/`, `service.rs`, `api.rs`, `mod.rs`
- `use_cases/` — Cross-cutting application use cases (placeholder)

**Frontend (`src/`)**:

- `bindings.ts` — Auto-generated from Rust via Specta (DO NOT EDIT)
- `features/{domain}/` — Feature modules (gold layout: `assets/`):
  - `gateway.ts` at root — only file allowed to call `commands.*`
  - Sub-feature subdirectories with colocated component + hook + test
  - `shared/presenter.ts` — domain → UI transformations; `shared/validate*.ts` — validation

**Data Flow**: Component → Hook → Gateway → Tauri Command → Rust Service → Repository

## 🥇 Gold Standards & Bit-by-Bit Trajectory

The project has three evolving "gold" targets the codebase moves toward **bit by bit** over time. Future sessions follow them for **new code** and for **small surgical updates** to existing code, but **never trigger a big-bang refactor** to make existing code conformant.

### The three golds

1. **Backend layout gold** — kit v4.4.0 (already shipped). Rules `B0`, `B37`–`B43` in `docs/backend-rules.md`. New code under `shared/` (not `core/`), `context/{bc}/{application,domain,infrastructure}/` symmetric trio, `infrastructure/` (not `repository/`).
2. **Frontend layout gold** — kit v4.9+ (already shipped). Rule `F0` in `docs/frontend-rules.md` defines the canonical `src/` tree (`features/`, `shell/`, `ui/`, `infra/` + framework exceptions `assets/` / `styles/` / `public/`); rule `F28` is the include/reject discipline that supports F0. New code follows F0; the existing `src/lib/` → `src/infra/` rename and the cross-feature import reframe are bit-by-bit migration targets. **Project divergence**: `App.tsx` stays at `src/` root (Vite/CRA convention) rather than moving to `src/shell/`; see [`docs/ddd-divergences.md`](docs/ddd-divergences.md) when adopted.
3. **Error-model gold** — landed. Canonical reference: [`docs/error-model.md`](docs/error-model.md). Per-BC `*ApplicationError::DatabaseError`; shared `InfrastructureError` was removed from the FE wire surface; application layer translates raw infra errors and logs server-side via `tracing::error!`. See also [`docs/ddd-divergences.md`](docs/ddd-divergences.md) for the intentional deviations from textbook DDD.

### Bit-by-bit update rule

Apply gold to **new code** (new files, new commands, new error variants, new features). For **existing code that touches a gold-standard area** during a task, fold gold conformance into the current task ONLY when ALL three hold:

- **Size**: ≤50 LOC of conformance changes (a checkpoint number, not a magic threshold — see "two stories" check below).
- **Locality**: changes stay within the natural file set the task already touches. Don't pull unrelated files into the diff just to gold-conform them.
- **Mechanical**: rename, import update, signature swap, type substitution. Any fresh **design judgement** ("which layer does this belong in?", "what should this variant be named?") triggers defer even if the line count is small — that's a design call that deserves its own PR + discussion.

If any of the three fails, **DO NOT refactor** — match the current project standard in the touched area and continue. The bigger gold migrations are tracked in `docs/techdebt.md` (e.g. "FE gold layout migration", "`core/` → `shared/` rename") and ratcheted in their own dedicated PRs when the user schedules them.

**The "two stories" sanity check** (overrides the LOC number when in tension): would a reviewer say this PR is telling **one story** (the feature/fix) or **two stories** (the feature/fix + a layout migration)? If two, the gold conformance IS the second story — defer it. The LOC threshold is just a fast-path heuristic for catching this; "two stories" is the real test.

**Why this rule**: gold consistency is a long-term ratchet, not a per-PR mandate. Refactor sprawl (touching unrelated files to make them gold-conformant) is the failure mode this policy prevents. The target of this app is shipping working features, not perfect layering. Each task pushes the codebase **a bit** closer to gold; never let "but it's not gold" block forward progress, and never let "while I'm here" balloon a task into a refactor.

**Consistency is not the goal**: if a touched area is currently using the OLD project standard and the surrounding code is OLD, KEEP IT OLD when conformance would breach the 50-LOC threshold. Mixed-standard codebase is acceptable during the bit-by-bit migration; pure gold conformance is acceptable too. What is NOT acceptable is partial-mid-flight refactors that leave neither standard intact.

**When in doubt** about whether something crosses the 50-LOC threshold: estimate, mention it in the task plan, ask the user. Don't silently drift into a big refactor.

## 📏 Standards

- **Commits**: Conventional commits (`feat:`, `fix:`, etc.).
- **Style**: React functional components, Rust traits for repositories.
- **Lints**: Oxlint & Biome (FE), Clippy (BE). All must pass.

## 🖼 Frontend Visual Proof

Full rules: `docs/frontend-visual-proof.md`

Any `.tsx`, `.css`, or visual asset change **must** include a committed screenshot in `screenshots/` before merging.

One-time setup: `npx playwright install chromium`

Run `/visual-proof` after any frontend change — auto-discovers config on first run, generates previews for all component states in light + dark mode, captures with Playwright, and stages screenshots.

> **Modals**: render the panel directly without `ModalContainer` in `src/__preview__/main.tsx` — copy the `FormModal` chrome (rounded-[28px], `bg-m3-surface-container-lowest/85 backdrop-blur-[12px] shadow-elevation-4`, header / scrollable content / footer) and skip `ModalContainer`'s 50% scrim. The scrim is a generic shell concern with no real content behind it in a standalone preview, so it would render near-black and misrepresent the modal in dark mode.

> **No visual change**: write `No visual impact — internal refactor / Rust-only change.` at the top of the PR/commit, then screenshot a screen that _consumes_ the modified code as non-regression proof.

## ⚠️ Critical Patterns

### Tauri Service Layer - Gateway Pattern

All Tauri invocations in services MUST match `bindings.ts` signatures EXACTLY:

- ✅ `commands.addAsset(name, assetClass, categoryId, currency, riskLevel, reference)` - positional parameters
- ❌ `commands.addAsset({ name, assetClass, categoryId, currency, riskLevel, reference })` - object wrap (WRONG)
- **Rule**: Match parameter COUNT, ORDER, and NAMES from bindings.ts
- When binding has 5 params: call with 5 args in correct order, never wrapped

### Domain Entities - Factory & Aggregate-Root Methods

Domain objects expose two distinct families of methods. NEVER construct them via direct
struct literals outside these conventions.

**Factories** — produce a fresh aggregate. Static, do not take `self`:

- `new()` — generates a new ID + validates input
- `with_id()` — uses a caller-supplied ID + validates input (services / use cases / api)
- `from_storage()` (or `restore()`) — reconstructs from the database, no validation
  (already validated at write time)

**Mutating aggregate-root methods** — apply a state-dependent change to a loaded
aggregate. Instance methods, take `self` (or `&mut self`):

- `update_from(self, …fields) -> Result<Self, DomainError>` — applies an edit; enforces
  state invariants then validates input; returns the updated aggregate to persist
- `archive(self) / unarchive(self) -> Result<Self, DomainError>` — flips the archive flag;
  enforces invariants
- `ensure_<predicate>(&self) -> Result<(), DomainError>` — fail-fast guard used when the
  rejection must precede an action that doesn't construct a new aggregate (e.g. delete)

Rules for this family:

- Use **domain/business vocabulary** (per `docs/backend-rules.md` B11) — name the
  business action (`archive`, `cancel`), not the mechanism (`set_archived(true)`)
- Return typed **domain errors** directly (per `docs/ddd-reference.md` § Errors) — never
  `anyhow`
- All **state-dependent rejections** (`Archived`, `CashAssetNotEditable`,
  `SystemReadonly`, `SystemProtected`, etc.) MUST live here — not in the service
- The repository ONLY uses factories, never direct struct literals

---

## 📋 Plan Format Guidelines

When proposing a TODO plan, Claude Code MUST:

- List exact file paths, not abstract locations.
- Name the specific functions/methods/components to create or modify.
- Separate clearly by architectural layer (backend / frontend / E2E / docs).
- Call out any gold-standard conformance work explicitly with its LOC estimate; if it's >50 LOC or fails the locality/mechanical gates, defer it and say so.
- Include validation and testing steps.
- Wait for explicit user approval before implementing.
