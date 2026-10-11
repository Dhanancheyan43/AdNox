<div align="center">

# AdNox

**A no-root ad & tracker blocker for Android and Windows.**

Blocks ads and trackers for *every app and every website* on your device by filtering DNS —
not just inside one browser. One fast filtering engine written in Rust, shared by both platforms.

</div>

---

## How it works

There is no single OS API that "blocks ads everywhere", so AdNox blocks at the **DNS layer**: when
any app tries to resolve an ad or tracker domain, AdNox answers with `0.0.0.0` (a *sinkhole*) so the
connection is never made. Legitimate domains are forwarded to your upstream resolver untouched.

```
                 ┌──────────────────────────────────────────┐
                 │              AdNox engine (Rust)           │
                 │   adnox-core  — filter-list parsing +      │
                 │                 request/domain matching    │
                 │                 (Brave's `adblock` engine) │
                 └───────────────┬───────────────┬────────────┘
                     JNI         │               │   C ABI
          ┌──────────────────────▼──┐     ┌──────▼─────────────────────┐
          │ Android                  │     │ Windows                    │
          │ VpnService captures DNS  │     │ adnox.exe runs on          │
          │ for all apps, no root    │     │ 127.0.0.1:53; system DNS   │
          │ (android/)               │     │ points at it (windows/)    │
          └──────────────────────────┘     └────────────────────────────┘
```

- **No root. No certificate.** DNS filtering needs neither.
- **Private by design.** On Android the "VPN" is local-only — nothing is sent to an AdNox server;
  AdNox does not decrypt your HTTPS traffic.
- **Open filter lists.** StevenBlack hosts, AdGuard DNS, EasyList, EasyPrivacy, Peter Lowe —
  all open-licensed and downloaded on demand. A small seed list ships in-app so it works offline.

## Repository layout

| Path | What |
|------|------|
| `engine/core` | The filtering engine — list parsing + DNS/URL matching. Pure Rust, fully tested. |
| `engine/dns`  | The local DNS server that sinkholes blocked domains and forwards the rest. |
| `engine/cli`  | `adnox` command-line tool (`update`, `run`, `test`, `lists`) — and the Windows service binary. |
| `engine/ffi`  | C ABI + JNI bridge (`libadnox`) with a C header at `engine/ffi/include/adnox.h`. |
| `android/`    | The Android app (`VpnService` + Kotlin UI) — see [android/README.md](android/README.md). |
| `windows/`    | Install/uninstall scripts + guide — see [windows/README.md](windows/README.md). |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | Design and data flow in detail. |
| [`ROADMAP.md`](ROADMAP.md) | Phased plan, including cosmetic filtering and packaging. |

## Try it in 60 seconds (any desktop OS)

The engine and DNS blocker build and run on Linux/macOS/Windows today:

```bash
cd engine
cargo build --release -p adnox-cli

# Does a domain get blocked? (uses the bundled seed list — no download needed)
./target/release/adnox test doubleclick.net      # -> BLOCKED
./target/release/adnox test example.com          # -> allowed

# Download the full lists and run the blocker on a test port
./target/release/adnox update
./target/release/adnox run --bind 127.0.0.1:5353 --upstream 1.1.1.1:53
# in another terminal:
dig @127.0.0.1 -p 5353 doubleclick.net   +short   # -> 0.0.0.0
dig @127.0.0.1 -p 5353 wikipedia.org     +short   # -> real address
```

## Building the apps

- **Android:** build `libadnox.so` with `cargo ndk`, then the APK with Gradle — full steps in
  [android/README.md](android/README.md).
- **Windows:** `cargo build --release -p adnox-cli`, then run `windows/install.ps1` as
  Administrator — full steps in [windows/README.md](windows/README.md).

## Status

This is an early, working **Phase 1** (DNS/network blocking). Cosmetic filtering (hiding leftover
blank ad boxes), IPv6/DoT handling, scheduled list updates, and signed installers are planned —
see [ROADMAP.md](ROADMAP.md). Contributions welcome; see [CONTRIBUTING.md](CONTRIBUTING.md).

## Legal

AdNox blocks connections to ad/tracker domains for the device's owner, using publicly available,
openly-licensed filter lists. It is intended for personal privacy and security. Respect the terms
of services you use. See [LICENSE](LICENSE) (MIT).
