# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> Full architecture reference: [ARCHITECTURE.md](ARCHITECTURE.md)

## 🧭 Behavioral Principles

Before coding:

- State assumptions explicitly. If multiple interpretations exist, present them — don't pick silently.
- If something is unclear, stop. Name what's confusing. Ask.

While coding:

- Every changed line must trace directly to the user's request.
- If you notice unrelated dead code, mention it — don't delete it.
- If 200 lines could be 50, stop and rewrite. Ask: "Would a senior engineer say this is overcomplicated?"
- **Surgical** — touch only what the task requires. Don't rewrite adjacent code "while you're here" beyond the bit-by-bit threshold (see § Gold Standards).
- **Gold unless not possible** — apply the gold standards (backend layout, frontend layout, error model) for new code and small surgical updates. When applying gold would breach the bit-by-bit threshold, match the current project standard in the touched area and continue.
- **Boyscout principle** — the touched area should leave a little better than you found it: collapse a duplicated helper, drop a now-redundant import, tighten a brittle pattern. Stay under the bit-by-bit threshold; if the cleanup balloons, defer it to its own PR.
- **No transition comments** — don't add tombstones like `// X was migrated to Y in PR N`. Git history carries the trail. Doc comments describe what the code IS, not what it USED TO BE.
- **Challenge reviewer findings** — reviewer agents (backend / frontend / arch / security) have false-positive rates. Before folding a finding, ask: is this a real issue in this codebase, or is it the reviewer applying a generic best-practice that doesn't fit the local context? Particularly applies to techdebt-class flags ("pre-existing tech debt", `[DECISION]` calls, suggestions to extract / generalise / add defensive code). Folding a genuinely-false-positive into `docs/techdebt.md` pollutes the backlog with non-issues that future `/whats-next` runs will surface as work. When unsure, push back in chat or ask the user before persisting.

## ⚠️ Workflow & Planning

See `.claude/kit-readme.md` for the full workflow guide and `.claude/kit-tools.md` for the agent/skill reference.

- **At session start**: run `/whats-next` to triage pending work across TODOs, plans, specs, and in-flight git.
  - When `/whats-next` identifies a ⚠️ likely-done item, immediately clean up the source doc (remove/cross off the entry in `docs/todo.md`, close open questions in specs, update the plan file, etc.) — do not just list it as a cleanup candidate.
- **After completing any action**: immediately update the source doc that tracked it — remove or tick off the entry in `docs/todo.md`, close the open question in the spec, tick the plan step, etc. Do not wait for the next `/whats-next` run.
- **At task start**: run `/start [scope]` (`fix`, `chore`, `test`, `feature`, `refactor`) to pick the right workflow.

**IMPORTANT**: Claude Code will NOT commit, create branches, or create PRs autonomously. Use `/create-pr` to push the current branch and open a GitHub PR (requires `gh` CLI). The user handles all git operations.

### CRITICAL: Implementation task

- Any code file is considered as implementation task
- ONLY exception is doc files
- Every task should follow _Plan Before Implementation_

### Project-specific workflow additions

On top of the standard kit workflow, this project requires:

1. **Before implementing**: read the relevant convention docs.
   - **Backend changes**: `docs/backend-rules.md` + `docs/ddd-reference.md` (especially when touching the error model — see § Errors → rejection-layer rule).
   - **Frontend changes**: `docs/frontend-rules.md` + `docs/i18n-rules.md`. Also read `docs/frontend-visual-proof.md`, then run `/visual-proof` after implementation to capture all states in both light and dark mode.
   - **E2E changes**: `docs/e2e-rules.md`.
   - **Any test work** (unit / integration / E2E, BE or FE): `docs/test_convention.md`.
2. **Plan step**: after proposing the TODO plan, immediately create a TaskList (`TaskCreate`) with one task per remaining step. Ask user to validate before implementing.
3. **Docs update**: at the end, update `ARCHITECTURE.md` if new files/modules added; `docs/todo.md` if new project backlog items or resolved items; for non-actionable code smells or reviewer-surfaced observations use `/techdebt` (output goes to `docs/techdebt.md`); for new business rules use `/spec-writer` to author/extend the spec in `docs/spec/` (then run the `spec-reviewer` agent to validate) and `/contract` to derive the matching `docs/contracts/{domain}-contract.md` (then run the `contract-reviewer` agent to validate). Use `/adr-writer` to author architectural decisions in `docs/adr/`, then run the `adr-reviewer` agent to validate before locking the decision.
4. **E2E tests** (after frontend impl, before release): run `test-writer-e2e` agent with the domain contract to write passing WebDriver E2E tests against the live app (verifies green before finishing). Run `/setup-e2e` once first if not yet initialized.
5. **Visual proof** (frontend changes only): run `/visual-proof` to capture and commit screenshots for all component states in both light and dark mode. **Modals: render the panel directly without `ModalContainer`** in `src/__preview__/main.tsx` — copy the `FormModal` chrome (rounded-[28px], `bg-m3-surface-container-lowest/85 backdrop-blur-[12px] shadow-elevation-4`, header / scrollable content / footer) and skip `ModalContainer`'s 50% scrim. The scrim is a generic shell concern with no real content behind it in a standalone preview, so it would render near-black and misrepresent the modal in dark mode. Visual proof is for the component, not the shell.
6. **Commit**: ask user if a commit is needed → use `/smart-commit` skill.

### Task tracking (within a conversation)

**MANDATORY** for every implementation task — use `TaskCreate` / `TaskUpdate`:

- Create tasks before implementing anything
- Mark each task `in_progress` when starting, `completed` when done

### PR strategy — split per layer for non-trivial features

For features that touch both backend and frontend, **default to one PR per layer** when either layer exceeds ~20 changed files or ~500 LOC. Below that threshold a single PR is fine.

When splitting, the order is **BE → FE → E2E**:

1. **Spec / contract / migration / backend domain + service + api + bindings** — first PR. Mergeable on its own (FE doesn't yet consume the new types but TS bindings are present and unused, no runtime impact).
2. **Frontend gateway / hooks / presenter / components / i18n** — second PR, branched off the merged BE branch. Reviewable against a stable backend.
3. **E2E tests + ARCHITECTURE / todo / spec-checker closure** — third PR.

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
2. **Frontend layout gold** — pending kit issues [#21](https://github.com/phileggel/claude-kit/issues/21), [#22](https://github.com/phileggel/claude-kit/issues/22), [#23](https://github.com/phileggel/claude-kit/issues/23) (FE cross-feature import reframe; canonical hook/presenter/component error-handling layering; `src/` folder mandates + `lib/` → `infra/` rename). Likely lands in kit v4.5+. Until then, follow the proposed shape from those issue bodies for new FE code.
3. **Error-model gold** — `docs/plan/error-model-refactor.md` § Locked rules → "Infra translation rule (project-specific tightening)". Per-BC `*ApplicationError::DatabaseError`; shared `InfrastructureError` does NOT appear on the FE wire surface; application layer translates raw infra errors and logs server-side via `tracing::error!`. Upstream proposal: kit issue [#28](https://github.com/phileggel/claude-kit/issues/28).

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

- List exact file paths, not abstract locations
- Name the specific functions/methods/components to create or modify
- Separate clearly by architectural layer (backend / frontend)
- Include validation and testing steps
- Wait for explicit user approval before implementing
