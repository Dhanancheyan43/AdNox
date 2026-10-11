# Contributing to AdNox

Thanks for your interest! AdNox is an open-source ad/tracker blocker. This guide covers the
day-to-day workflow.

## Prerequisites

- **Rust** (stable) via <https://rustup.rs> — required for the engine, DNS server, CLI and FFI.
- For the Android app: **Android Studio** (or the SDK + NDK), plus `cargo-ndk` and the Android Rust
  targets. See [android/README.md](android/README.md).
- For Windows packaging: PowerShell 5+ (built in).

## Build & test the engine

```bash
cd engine
cargo build
cargo test            # all crates
cargo fmt --all       # format
cargo clippy --all-targets --all-features -- -D warnings
```

CI runs `fmt --check`, `clippy -D warnings`, and `test` on every push/PR; please run them locally
first.

## Project conventions

- Keep **all filtering logic in `adnox-core`** so both platforms stay in sync; platform code should
  only do packet/transport plumbing and UI.
- FFI boundary functions must null-check arguments and wrap work in `catch_unwind`.
- Add a unit test with any behavioural change to the engine.
- Match the surrounding style; prefer small, focused functions.

## Filter lists

AdNox ships only openly-licensed public lists and downloads them from their canonical sources. If
you add a default source, note its license in the PR.

## Pull requests

1. Branch from the default branch.
2. Keep PRs focused; describe the change and how you tested it.
3. Make sure `cargo test`, `fmt`, and `clippy` pass.

## Reporting issues

Please include your platform, AdNox version, steps to reproduce, and (for blocking/over-blocking
reports) the exact domain or URL involved.
