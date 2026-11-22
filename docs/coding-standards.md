# Coding guidelines

See [AGENTS.md](../AGENTS.md) for product constraints, [architecture](architecture.md) for ownership, and [testing](testing.md) for checks.

## Boundaries

- Commands validate arguments and call services; domain modules never depend on commands. DAOs own SQL and transactions.
- Only `src/lib/api.ts` invokes Tauri commands. Preserve typed DTOs and flat camelCase IPC arguments.
- TanStack Query holds backend snapshots; Zustand holds interaction state. Centralize library-scoped query keys and reject stale results after switching libraries.
- Reuse shared sessions, activity guards, and queue context. Release listeners/timers on unmount. Never hold synchronous locks across await or slow work inside database write locks.
- File, SQLite, and vector operations need explicit commit points, recovery, and idempotent retries. Do not treat a committed database write as rolled back after a vector failure. Never modify external reference sources.

## Implementation

Use English for comments, diagnostics, and developer documentation; preserve localized UI text and Unicode fixtures. Keep TypeScript strict: no new `any`, `@ts-ignore`, unexplained assertions, or lint suppressions. Validate external input, follow hook dependency rules, and handle promises and background errors explicitly.

Rust uses rustfmt and stable AppResult/boundary errors. Reuse domain types and keep model/IPC contracts aligned. Comments explain constraints; remove obsolete debugging code. Tests belong outside business source; private Rust modules may declare external test modules.

## Quality gates

Use Node.js 22.18+ and the pinned CI toolchain (Rust 1.99.0 with rustfmt/Clippy). Format touched frontend files with Prettier and Rust with rustfmt; preserve unrelated changes.

Frontend business files are limited to 500 lines and Rust business files to 700. Split by responsibility, not by compressing code. Locale dictionaries and environment declarations are exempt. Keep every exception map in `.quality-baseline.json` empty: do not raise limits, suppress diagnostics, weaken tests, or add baseline entries.

| Command                 | Purpose                                                     |
| ----------------------- | ----------------------------------------------------------- |
| `npm run quality:check` | Tool tests, lint, formatting, architecture, and file limits |
| `npm run quality:rust`  | All Rust targets, formatting, and Clippy                    |
| `npm run quality`       | Quality gates plus frontend typecheck, tests, and build     |

Run the [required checks](testing.md) and related behavior tests before submitting. Quality gates do not prove concurrency safety or hardware performance. Report actual test execution and unverified platforms; update affected documentation.
