
# AGENTS.md — DocVault AI Development Guide

This document defines how AI agents (Codex / automated coding assistants / contributors) should interact with the DocVault codebase.

It focuses on consistency, maintainability, and safe incremental development.

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

------

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

------

## 1.3 Naming Conventions

### Rust

- snake_case → functions, variables
- PascalCase → structs, enums, traits
- UPPER_SNAKE_CASE → constants
- Modules → snake_case

### TypeScript

- camelCase → variables, functions
- PascalCase → components, types, interfaces

### Domain Naming Preference

Prefer domain-oriented naming:

✔ Recommended:

- Document
- Version
- ImportJob
- RestoreJob

✖ Avoid:

- FileManagerV2
- DocHandlerUtil
- ManagerServiceHelper

------

# 2. Project Structure Rules

## 2.1 Monorepo Layout

```
crates/
  core/        → business logic (import / restore / versioning)
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

------

## 2.2 Layering Rules

### Allowed dependencies

```
UI (cli/desktop)
  ↓
core
  ↓
storage / ooxml / jobs
```

### Strict rules

- core MUST NOT depend on apps/
- storage MUST NOT depend on UI
- ooxml MUST NOT depend on storage
- jobs MUST NOT depend on UI

------

## 2.3 Core module responsibility

core is responsible for:

- Document lifecycle
- Version management
- Job orchestration
- Business rules

core is NOT responsible for:

- File system implementation details
- Restic CLI execution
- UI state

------

## 2.4 Third-party runtime assets

Restic is a bundled runtime dependency for v1, not application source code.

Recommended layout:

```text
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
      aarch64-unknown-linux-gnu/
        restic
      x86_64-apple-darwin/
        restic
      aarch64-apple-darwin/
        restic
```

Rules:

- Do not place platform binaries in the repository root.
- Keep Restic binaries grouped by upstream version and Rust target triple.
- Keep checksum and license information next to the bundled binaries.
- Desktop packaging may copy the selected binary into `apps/desktop/src-tauri/binaries/`.
- CLI packaging should use the same `third_party/restic` source asset.
- Do not implement multiple Restic discovery mechanisms in different crates.

------

# 3. AI Editing Principles

## 3.1 Primary rule: reuse before creating

Before creating new modules:

- Search existing crates for similar logic
- Extend existing functions where reasonable
- Prefer composition over duplication

------

## 3.2 Minimal change principle

When modifying code:

- Keep changes localized
- Avoid refactoring unrelated modules
- Avoid restructuring directories unless required

------

## 3.3 No speculative architecture

Agents must avoid:

- Adding plugin systems
- Introducing abstraction layers not used yet
- Designing cloud interfaces prematurely
- Over-generalizing storage or backend logic

If a feature is not explicitly required → do not implement.

------

## 3.4 Prefer explicit over abstract

Prefer:

```rust
StorageService::backup_version()
```

Avoid:

```rust
trait StorageBackend
```

unless explicitly requested.

------

## 3.5 Preserve working flows

If a feature works:

- Do not optimize prematurely
- Do not refactor for style alone
- Do not split working functions unnecessarily

------

# 4. Testing Requirements

## 4.1 Required test types

### Unit tests (Rust)

Each crate must include:

- core business logic tests
- storage operation tests (mocked)
- ooxml parsing tests

Run:

```bash
cargo test
```

------

### Integration tests

Must cover:

- Import workflow
- Restore workflow
- SQLite persistence consistency
- Restic snapshot creation (mock or test repo)

Run:

```bash
cargo test --tests
```

------

### CLI tests

Basic CLI behavior must be verified:

```bash
cargo run --bin docvault -- import ./sample.docx
cargo run --bin docvault -- list
cargo run --bin docvault -- restore <id>
```

------

### Frontend tests (if applicable)

```bash
npm run test
```

------

## 4.2 Required pre-merge checks

All changes must pass:

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

------

# 5. Runtime Constraints

## 5.1 Environment assumptions

The system runs in:

- Local desktop environment
- macOS / Windows / Linux
- No external services required in v1

------

## 5.2 Required environment variables

```text
DOCVAULT_DATA_DIR
DOCVAULT_DB_PATH
DOCVAULT_LOG_LEVEL
```

If not set, defaults are used from config.toml.

------

## 5.3 File system rules

- All original files are immutable after import
- Temporary files must be stored in staging directory
- Restore operations must write to explicit output path

------

## 5.4 Restic execution rules

Restic is executed via CLI process:

- All commands must be deterministic
- Output must be captured and parsed
- Errors must be propagated explicitly
- No hidden state is allowed
- Restic path resolution must be explicit and testable
- Prefer this lookup order:
  1. configured `restic_path`
  2. `DOCVAULT_RESTIC_PATH`
  3. packaged application sidecar
  4. development asset under `third_party/restic/<version>/<target>/`
  5. `restic` from system `PATH`
- Keep Restic command construction in storage-layer code, not UI code.
- Avoid introducing a generic plugin or backend abstraction unless explicitly requested.

------

## 5.5 Mocking rules (for tests)

- Restic must be mocked in unit tests
- SQLite may use in-memory DB for tests
- File system interactions must use temp directories

Example:

```rust
tempfile::TempDir
```

------

# 6. Logging & Debugging Rules

## 6.1 Logging system

All modules must use:

```rust
tracing::info!
tracing::debug!
tracing::error!
```

------

## 6.2 Log requirements

- Every job must emit start/end logs
- Every failure must include context
- No silent failures allowed

------

# 7. Performance Guidelines

- Prefer streaming over full file loading
- Avoid cloning large buffers unnecessarily
- Use async only where IO-bound
- Keep CPU-heavy logic synchronous unless necessary

------

# 8. Error Handling Rules

- Use Result<T, E> everywhere in core
- Avoid panics in production code paths
- Convert external errors into domain errors

------

# 9. What AI Agents SHOULD do

- Extend existing modules
- Keep logic simple and readable
- Follow existing patterns in codebase
- Add tests when introducing new behavior

------

# 10. What AI Agents MUST NOT do

- Introduce new architectural layers
- Add unused abstractions
- Create multiple competing implementations
- Modify working pipelines without reason
- Introduce cloud or distributed assumptions

------

# 11. Design Stability Principle

The system prioritizes:

> Working local-first functionality over theoretical extensibility

Any change must preserve:

- Deterministic behavior
- Recoverability of data
- Compatibility with existing imports and versions

------

