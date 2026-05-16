# ADR 011 — User-supplied API keys via OS keychain with layered Linux fallback

**Date**: 2026-05-16
**Status**: Accepted

## Context

ADR-008 introduces Finnhub as a price-fallback that requires a user-supplied free API key. The same pattern is anticipated for OpenFIGI (free key lifts the WEB lookup search rate from 5 req/min to ~100 req/min) and for future paid-tier providers. The decision needed is how the app stores those keys.

Two non-negotiables frame the decision:

1. **No key may ever be bundled in the app binary.** Anything shipped in the build is trivially extracted with `strings` or any reverse-engineering tool. The model is **BYOK (bring-your-own-key)**: the user signs up at the provider themselves and pastes the key into VaultCompass.
2. **Keys live on the user's machine only.** VaultCompass is offline-first and does not operate a backend proxy. The app never transmits keys to anywhere except the intended provider.

Given those constraints, the question is how local storage is implemented so that the key is not casually readable from disk, backups, cloud-sync folders, or crash dumps.

## Decision

Use the OS keychain via the Rust `keyring` crate as the **default** storage, with a **three-tier fallback ladder** detected at runtime in priority order. Tier 1 wraps multiple backends the `keyring` crate selects transparently, so from the application's perspective there is a single OS-keychain call before any fallback decision:

1. **OS keychain via `keyring` crate** — macOS Keychain, Windows Credential Manager, Linux Secret Service (GNOME Keyring / KWallet / KeePassXC) **and** Freedesktop portal `org.freedesktop.portal.Secret` on newer Linux. The crate picks the backend at runtime; the app sees one API. The OS handles encryption and ties access to the logged-in user session. This is the default on every supported platform.
2. **Session-only in-memory storage** — when tier 1 fails to initialise (typically minimal Linux WMs without any keyring service or portal). The user pastes the key at app launch; it is held in process memory and never written to disk. Cleared on app exit.
3. **Explicit-opt-in plaintext file** — gated behind a confirmation dialog with warning copy. The user must check a box acknowledging the risk. This tier exists as an escape hatch for users who consciously trade security for convenience.

Three rules accompany the tier ladder, enforced by the forthcoming KEY spec:

- Keys MUST NEVER appear in logs, error messages, crash reports, or any telemetry payload. Treated as a hard exclusion in any logging/reporting code.
- Removing a key MUST clear every tier the app could have written to, not just the active one. Prevents resurrection from a stale fallback file.
- The active storage tier MUST be surfaced in the Connections settings panel so the user always knows where their key lives.

Alternatives considered:

- **`tauri-plugin-stronghold`** (Tauri's official encrypted-vault plugin, based on IOTA Stronghold) — rejected. Adds a required master-password prompt every launch, which is the wrong UX for an indie portfolio app where the threat model is "casual filesystem browse," not "high-value enterprise secret." The OS keychain already authenticates the user via OS login.
- **Encrypted SQLite column with machine-derived key** — rejected. The derivation function lives in the app binary, so anyone with the binary + SQLite file can reproduce the key. Security theater; "encrypted" in name only against the casual-browse threat.
- **Plaintext storage as default** — rejected. Leaks through backups, cloud-sync folders, casual file browsing, and any logging accident. Acceptable only behind explicit opt-in (tier 3), never as the silent default.
- **VaultCompass-operated proxy server holding shared keys** — rejected. Requires running infrastructure, exposes the operator to upstream API costs, defeats the offline-first ethos, and creates a central honeypot for all users' usage data.
- **OAuth per provider** — rejected as not available. The free-tier financial APIs in scope (Finnhub, OpenFIGI, Twelve Data) only issue static API keys; OAuth is not on offer.

## Consequences

- **Pros**: OS-level encryption on every supported platform; no master password prompt (the user's OS login is the authentication); cross-platform with a single Rust dependency; no key ever lives in the app binary; no key ever lives on a VaultCompass-operated server; the three-tier ladder degrades gracefully on minimalist Linux setups without forcing the user into plaintext.
- **Cons**: tier-2 (session-only) costs the user one paste per launch on Linux without a keyring; tier-3 (opt-in plaintext) creates a code path that must remain visibly gated and well-documented to avoid security regression; the Connections panel and tier-detection diagnostics add UI surface that must be maintained alongside provider additions; uninstalling the app does not necessarily clear OS-keychain entries (the user must remove them via Keychain Access / Credential Manager / Seahorse — or via the app's "Remove key" button before uninstall).
