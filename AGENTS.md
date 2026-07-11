# Repository Guidelines

## Project Structure & Module Organization

Easy Package is a macOS-first, read-only development-environment inspector built with Tauri 2.

- `src/` contains the React 19 + TypeScript UI: pages, components, API access, and hooks.
- `src/mock-data.ts` supplies browser-preview data when Tauri is unavailable.
- `src/lib/` contains small, framework-independent helpers and their tests.
- `src-tauri/src/` is the Rust backend: `commands.rs` exposes Tauri commands, `adapters/` discovers package managers, `scan/` inspects projects and health, and `storage.rs` persists local SQLite data.
- `src-tauri/icons/` holds application icons. Do not edit generated `dist/`, `src-tauri/target/`, or `src-tauri/gen/` output by hand.

## Build, Test, and Development Commands

Use pnpm 11 as declared in `package.json`.

```bash
pnpm install                         # Install JavaScript dependencies
pnpm dev                             # Run the Vite UI with mock data
pnpm tauri dev                       # Run the full desktop app
pnpm test                            # Run Vitest tests once
pnpm build                           # Type-check and build the frontend
pnpm check                           # Frontend tests/build plus Rust tests
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
```

## Coding Style & Naming Conventions

Follow the local style: two-space indentation and double quotes in TypeScript; `PascalCase` component names; `camelCase` functions, hooks, and variables; `*.test.ts(x)` beside the module or in its feature directory. Prefer explicit TypeScript domain types from `src/types.ts`; do not introduce `any` to silence errors.

Use idiomatic Rust formatting (`cargo fmt`) and `snake_case` for modules, functions, and fields. Keep Tauri commands thin; place command execution under `adapters/`, scanning under `scan/`, and persistence under `storage.rs`. Backend commands must remain read-only and must not invoke a shell.

## Testing Guidelines

Use Vitest with Testing Library and assert user-visible behavior. Add regression tests for bug fixes and cover changed filtering, empty/error states, and command parsing. Rust unit tests belong in the affected module under `#[cfg(test)]`; use temporary directories for filesystem behavior. Run the narrowest relevant test first, then `pnpm check`.

## Commit & Pull Request Guidelines

Git history is not present in this checkout, so use concise imperative commits such as `feat: add uv cache diagnostics`. Keep commits focused. PRs should describe behavior changes, validation commands run, related issues, and screenshots for UI changes. Call out scan scope, storage, command execution, or permission changes explicitly.

## Security & Configuration

Never add arbitrary command execution or secret logging. Homebrew Formula and pnpm global-package mutations must go through the existing one-time action-plan registry, manager-specific trusted executable validation, fixed argument builders, explicit confirmation, operation mutex, timeout/cancellation handling, post-action rescan, interrupted-action recovery, and local audit record. pnpm must keep lifecycle scripts disabled and reject version, URL, Git, and local-path install sources. Do not extend write access to another manager without equivalent tests and an explicit product decision. Keep scan commands read-only, use executable paths plus argument arrays, and preserve output redaction.
