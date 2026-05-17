# Portfolio Manager — Command Runner
# Install just: https://github.com/casey/just

import "common.just"

# List all available commands
default:
    @just --list

# Install all dependencies
install:
    npm install

# Start the application with hot reload
dev *ARGS:
    ./scripts/start-app.sh {{ARGS}}

# Start the application using Wayland backend (use when window doesn't appear with just dev)
dev-wayland *ARGS:
    GDK_BACKEND=wayland ./scripts/start-app.sh {{ARGS}}

# Regenerate Specta TypeScript bindings (run after adding or changing Tauri commands)
generate-types:
    cd src-tauri && cargo run --bin generate_bindings --features generate-bindings

# Run frontend tests
test:
    npm test

# Run backend tests
test-rust:
    cd src-tauri && cargo test

# Run frontend tests with lcov coverage (output: coverage/frontend/lcov.info)
coverage-fe:
    npm run test:coverage

# Run backend tests with coverage (output: coverage/backend/lcov.info + tarpaulin-report.html); requires: cargo install cargo-tarpaulin
coverage-be:
    cd src-tauri && cargo tarpaulin --out Lcov Html --output-dir ../coverage/backend --lib --exclude-files "build.rs" --exclude-files "src/bin/generate_bindings.rs"

# Run E2E tests against the built binary (opens a window)
test-e2e:
    npm run test:e2e

# Run E2E tests headlessly via Xvfb (Linux / CI, no display required)
test-e2e-headless:
    npm run test:e2e:xvfb

# Run unit tests only (excludes E2E and coverage; see test-e2e and coverage-fe/coverage-be)
test-unit: test test-rust

# Collect logs for debugging
collect-logs:
    ./scripts/collect-logs.sh

# Take a screenshot of the app
screenshot:
    ./scripts/screenshot.sh

# Run linters only
lint:
    npm run lint
    cd src-tauri && cargo clippy -- -D warnings

# Clean build artifacts
clean:
    rm -rf dist src-tauri/target

# ⚠️  Destructive: resets database and restarts app in dev mode
reset-db:
    ./scripts/start-app.sh --reset-db

# Run pending database migrations. Override `URL=...` to target a different DB.
db-migrate URL="sqlite:.local/dev_check.sqlite":
    cd src-tauri && DATABASE_URL={{URL}} sqlx migrate run
