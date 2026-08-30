# AGENTS.md — DocVault AI Development Guide

This document defines how AI agents (Codex / automated coding assistants / contributors) should interact with the DocVault codebase.

It focuses on consistency, maintainability, safe incremental development, and preserving a clear domain-oriented architecture.

---

# 1. Code Style & Quality Rules

## 1.1 Formatting

All Rust code must be formatted using:

```bash
cargo fmt
```

All TypeScript/Frontend code must follow Prettier formatting:

```bash
npm run format
```

Formatting is considered part of every change.

---

## 1.2 Linting

### Rust

```bash
cargo clippy --all-targets --all-features
```

Clippy warnings should be resolved or explicitly justified.

### Frontend

```bash
npm run lint
```

ESLint rules must be respected.

---

## 1.3 Naming Conventions

### Rust

* snake_case → functions, variables
* PascalCase → structs, enums, traits
* UPPER_SNAKE_CASE → constants
* Modules → snake_case

### TypeScript

* camelCase → variables, functions
* PascalCase → components, types, interfaces

### Domain Naming Preference

Prefer domain-oriented naming.

Recommended:

```
Document
Version
CommitJob
ExportJob
CheckoutJob
```

Avoid vague technical names:

```
FileManagerV2
DocHandlerUtil
ManagerServiceHelper
CommonHelper
```

---

## 1.4 Commit Message Conventions

Use Conventional Commits style:

```text
type(scope): short imperative summary
```

Rules:

* Allowed types: `feat`, `fix`, `refactor`, `chore`, plus `ci`, `docs`,
  `test` when they describe the whole change.
* `scope` is the affected module or feature area, such as `desktop`,
  `qinbixin`, `addin`, `installer`, `i18n`, `core`, `storage`, or `cli`.
  It is not a file path.
* Omit the scope only for genuinely cross-cutting changes.
* Keep the summary short, imperative, and specific. Use the optional body
  to explain why, not what.

Do not rewrite pushed history only to reformat old commit messages.
Follow this convention for new commits.

---

# 2. Project Structure Rules

## 2.1 Monorepo Layout

```
crates/
  core/        → business logic (commit / export / checkout / versioning)
  storage/     → SQLite + Restic integration
  ooxml/       → OOXML parsing + manifest generation
  jobs/        → async job system

apps/
  cli/         → command line interface
  desktop/     → Tauri application

shared/
  types/       → shared domain models

third_party/
  restic/      → bundled Restic binaries, grouped by version and target triple
```

---

## 2.2 Layering Rules

Allowed dependency direction:

```
UI (cli/desktop)
  ↓
core
  ↓
storage / ooxml / jobs
```

Strict rules:

* core MUST NOT depend on apps/
* storage MUST NOT depend on UI
* ooxml MUST NOT depend on storage
* jobs MUST NOT depend on UI

---

## 2.3 Core Module Responsibility

core is responsible for:

* Document lifecycle
* Version management
* Job orchestration
* Business rules

core is NOT responsible for:

* File system implementation details
* Restic CLI execution
* UI state management

---

## 2.4 Third-party Runtime Assets

Restic is a bundled runtime dependency for v1, not application source code.

Recommended layout:

```
third_party/
  restic/
    <version>/
      manifest.toml
      checksums.txt
      licenses/
      x86_64-pc-windows-msvc/
        restic.exe
      x86_64-unknown-linux-gnu/
        restic
```

Rules:

* Do not place platform binaries in repository root.
* Keep Restic binaries grouped by upstream version and Rust target triple.
* Keep checksum and license information next to bundled binaries.
* Desktop packaging may copy selected binaries into Tauri sidecar location.
* CLI packaging should reuse the same third_party/restic assets.
* Do not implement multiple Restic discovery mechanisms.

---

# 3. AI Editing Principles

## 3.1 Primary Rule: Reuse Before Creating

Before creating new modules:

* Search existing crates for similar logic.
* Extend existing functionality when responsibility is the same.
* Prefer composition over duplication.
* Check existing dependencies before implementing custom utilities.

Do not create:

* duplicate helpers
* alternative implementations
* unnecessary wrapper layers

---

## 3.2 Minimal Change Principle

When modifying code:

* Keep changes localized to the affected domain.
* Avoid unrelated refactoring.
* Avoid restructuring directories without reason.

However:

* Do not avoid necessary refactoring when existing structure prevents maintainability.
* Adding a new module is preferred over significantly expanding an unrelated existing module.
* Minimal change means minimal conceptual disruption, not minimum number of files changed.

---

## 3.3 No Speculative Architecture

Agents must avoid:

* Adding plugin systems.
* Introducing unused abstraction layers.
* Designing cloud interfaces prematurely.
* Creating generic backend systems before they are required.
* Generalizing code for hypothetical future requirements.

If a feature is not explicitly required:

Do not implement it.

---

## 3.4 Prefer Explicit Over Abstract

Prefer:

```rust
StorageService::backup_version()
```

Avoid:

```rust
trait StorageBackend
```

unless multiple implementations are actually required.

Do not introduce abstraction only for theoretical flexibility.

---

## 3.5 Preserve Working Flows

If a feature works:

* Do not optimize prematurely.
* Do not refactor only for cosmetic reasons.
* Do not split code without improving readability, testing, or responsibility boundaries.

However:

* Split modules when responsibilities become unclear.
* Extract code when it improves maintainability or testing.
* Do not allow working code to become an unmaintainable monolith.

---

## 3.6 File and Module Organization

The codebase should remain modular and maintainable.

### Single Responsibility

Each file and module should have one clear responsibility.

Avoid placing unrelated:

* business logic
* database operations
* filesystem handling
* CLI/UI code
* utility functions

inside the same file.

---

### File Size Guidelines

Prefer files under approximately 300-500 lines.

A larger file is acceptable only when:

* responsibilities are tightly related
* splitting would reduce readability
* the file represents a coherent domain unit

When a file grows large, evaluate extracting:

* independent workflows
* domain services
* adapters
* data models
* helper modules

Do not split files artificially only to reduce line count.

---

### Avoid God Files

Avoid files such as:

```
utils.rs
helpers.rs
manager.rs
service.rs
common.rs
```

that accumulate unrelated logic.

Bad:

```
storage.rs

- SQLite operations
- Restic execution
- filesystem helpers
- backup workflow
- export workflow
```

Preferred:

```
storage/
  mod.rs
  sqlite.rs
  restic.rs
  repository.rs
```

---

## 3.7 Domain-Oriented Module Organization

Prefer organizing code around business concepts.

Preferred:

```
core/
  document/
    commit.rs
    export.rs
    checkout.rs
    version.rs

  jobs/
    commit_job.rs
    export_job.rs
    checkout_job.rs
```

Avoid organizing only by technical categories:

```
core/
  utils.rs
  helpers.rs
  managers.rs
  services.rs
```

A module should represent a meaningful domain concept.

---

## 3.8 Rust Module Organization

Prefer Rust module trees over large flat files.

Preferred:

```
storage/
  mod.rs
  sqlite.rs
  restic.rs
  repository.rs
```

Avoid:

```
storage.rs
```

containing all storage implementations.

`mod.rs` should primarily define:

* module structure
* exports
* interfaces

Avoid placing large implementations inside `mod.rs`.

---

## 3.9 Refactoring Triggers

Agents should consider refactoring when:

* A file contains multiple unrelated responsibilities.
* A module mixes domain logic and infrastructure logic.
* A function becomes difficult to understand or test.
* Adding a feature requires navigating a very large file.
* Tests are difficult because a module does too many things.

Do not wait until architecture becomes difficult to change.

---

## 3.10 Dev-Only Code Gating

Development-only functionality (test backends, dev accounts, seed/reset
actions, dev tooling) must be excluded from release builds at compile time.
Never ship dev code guarded only by a runtime flag.

### Rust

* Gate dev-only items (functions, commands, constants, modules) with
  `#[cfg(debug_assertions)]`; provide the production variant with
  `#[cfg(not(debug_assertions))]` when a counterpart is required.
* NEVER use runtime ifs such as `if cfg!(debug_assertions)`. They leave the
  dev branch in the source and invite accidental edits to production paths.
* Register dev-only Tauri commands in `generate_handler!` behind the same
  `#[cfg(debug_assertions)]` attribute (see the Qinbixin dev commands).
* Build scripts (`build.rs`) may inject single-value differences
  (e.g. `cargo:rustc-env`) only when a branch-free single code path is
  genuinely required; prefer cfg-split functions.

### Frontend

* Gate dev-only UI and behavior with `import.meta.env.DEV`. Vite statically
  replaces it with `false` and tree-shakes the dead branch in production
  builds, so gated code never ships.
* Do not invent custom runtime dev flags for build-time gating.

### Runtime Toggles Are Not Dev Gating

A user-facing toggle persisted at runtime (e.g. a localStorage-backed
preference) is a product decision, not build-time gating. If such a toggle
ships, it must be intentional and documented; do not use it to smuggle
dev/test functionality into release builds.

### Verification

When touching dev-gated code, confirm the release output is clean:

* Frontend: build once and confirm dev-only capabilities are unreachable -
  gated UI never renders and gated `invoke()` calls target backend commands
  that release builds do not register. Small inert string references inside
  the JS bundle are acceptable; unreachable backend commands are what matter.
* Rust: `cargo build --release` binaries must not contain dev-only strings
  (use `strings` or an equivalent search when unsure).

---

# 4. Testing Requirements

## 4.1 Required Test Types

### Unit Tests (Rust)

Each crate should include:

* core business logic tests
* storage operation tests (mocked)
* ooxml parsing tests

Run:

```bash
cargo test
```

---

## 4.2 Integration Tests

Must cover:

* Commit workflow
* Export workflow
* Checkout workflow
* SQLite persistence consistency
* Restic snapshot creation (mock or test repository)

Run:

```bash
cargo test --tests
```

---

## 4.3 CLI Tests

Verify:

```bash
cargo run --bin docvault -- commit ./sample.docx --name sample
cargo run --bin docvault -- list
cargo run --bin docvault -- export sample --version latest --output ./out
cargo run --bin docvault -- checkout sample --version v1
```

---

## 4.4 Frontend Tests

If applicable:

```bash
npm run test
```

---

## 4.5 Required Pre-merge Checks

Rust:

```bash
cargo fmt
cargo clippy
cargo test
```

Frontend:

```bash
npm run lint
npm run test
```

---

# 5. Runtime Constraints

## 5.1 Environment Assumptions

The system runs in:

* Local desktop environment
* macOS / Windows / Linux
* No external services required in v1

---

## 5.2 Configuration Sources

The vault reads configuration only from:

* the on-disk `config.toml` (written at `init`), and
* explicit parameters supplied by the caller (CLI flags such as
  `--root-dir` / `--restic-path`, or the desktop's in-process
  bundled-restic path).

No `DOCVAULT_*` environment variables are read. The process environment
is not a configuration channel; this avoids silent, easily-overlooked
overrides.

---

## 5.3 File System Rules

* Original files are immutable after commit.
* Temporary files must be stored in staging directory.
* Export operations must write to explicit output paths.
* Database must not persist local filesystem paths.
* Persist original_filename instead of source paths.

---

## 5.4 Restic Execution Rules

Restic is executed through CLI process.

Rules:

* Commands must be deterministic.
* Output must be captured and parsed.
* Errors must propagate explicitly.
* No hidden state.
* Restic path resolution must be explicit and testable.

Lookup order:

1. explicit parameter (CLI `--restic-path`, or the desktop's bundled-restic injection)
2. configured `restic_path` in config.toml
3. packaged application sidecar
4. third_party/restic asset
5. system PATH

Keep Restic command construction inside storage layer.

Do not create plugin or backend abstractions unless explicitly required.

---

## 5.5 Mocking Rules

Tests:

* Restic must be mocked in unit tests.
* SQLite may use in-memory DB.
* File operations should use temporary directories.

Example:

```rust
tempfile::TempDir
```

---

# 6. Logging & Debugging Rules

## 6.1 Logging System

Use:

```rust
tracing::info!
tracing::debug!
tracing::error!
```

---

## 6.2 Log Requirements

* Every job emits start/end logs.
* Every failure includes context.
* No silent failures.

---

# 7. Performance Guidelines

* Prefer streaming over full file loading.
* Avoid unnecessary cloning of large buffers.
* Use async only for IO-bound operations.
* Keep CPU-heavy logic synchronous unless required.

---

# 8. Error Handling Rules

* Use Result<T, E> in core logic.
* Avoid panics in production paths.
* Convert external errors into domain errors.

---

# 9. What AI Agents SHOULD Do

* Extend existing modules.
* Search existing code before creating new implementations.
* Keep logic simple and readable.
* Follow existing architectural patterns.
* Add tests for new behavior.
* Extract modules when responsibilities diverge.

---

# 10. What AI Agents MUST NOT Do

* Introduce unnecessary architectural layers.
* Add unused abstractions.
* Create duplicate implementations.
* Append unrelated logic into existing large files.
* Create generic utility modules as dumping grounds.
* Modify working pipelines without reason.
* Introduce cloud or distributed assumptions.

---

# 11. Design Stability Principle

The system prioritizes:

> Working local-first functionality over theoretical extensibility.

Any change must preserve:

* Deterministic behavior.
* Data recoverability.
* A clear current schema and command model.

DocVault is in early active development. Do not add compatibility shims, aliases, or migrations
for obsolete experimental commands or schemas unless explicitly requested. Prefer updating docs
and tests to the current model and recreating local test vaults after breaking schema changes.

---
