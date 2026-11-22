# Architecture

An independent project retaining ClipSeek's desktop interface and application infrastructure.

| Layer                            | Responsibility                                            |
| -------------------------------- | --------------------------------------------------------- |
| React / Tailwind / Radix         | Interface, previews, settings, and progress               |
| Zustand / TanStack Query         | Interaction state / library-scoped backend snapshots      |
| `src/lib/api.ts` / Rust commands | Typed IPC and argument validation                         |
| LibrarySession / services        | Shared sessions, activity guards, and business operations |
| Import / indexing workers        | Durable queues, recovery, cancellation, and source checks |
| SQLite / Qdrant Edge             | Metadata, vector indexes, filtering, and retrieval        |
| ClipHandle / embedding           | Shared encoder, model lifecycle, and inference            |
| rmcp                             | Optional authenticated MCP access to the same services    |

Imports reference original files. Each photo or video has one normalized 768-dimensional vector; videos longer than 32 seconds or with unknown duration are rejected. Model-required video inputs are decoded in memory for one joint embedding, without a persisted frame search index. The internal `frames` table retains one row per asset for existing SQL/vector contracts.

The model adapter supports Q8 by default, optional Q4/FP32, and CoreML/Metal/CPU placement. See [inference](inference.md). Precision changes require idle workers, invalidate old vectors, clear encoder/query caches, and requeue indexing. A backend change preserves model identity.

Library-scoped state, session guards, queue ownership, and stale-result protection apply to desktop and MCP operations. Source files are never modified by library deletion. Playback uses a local HTTP Range service with session lifetime guards.

Independent configuration and library markers isolate this project from the original application. Online downloads, trimming, subtitles/ASR, licensing, plugin downloads, telemetry, and automatic updates are excluded. Legacy database fields remain only where migrations or query compatibility require them.
