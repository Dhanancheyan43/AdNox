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

## How to test it

Three ways, from fastest to most complete. **All of them start with the one-time setup below.**

### Step 0 — One-time setup (any OS)

1. **Install Rust** from <https://rustup.rs> (on Windows, download and run `rustup-init.exe`).
2. **Get the code and build** the `adnox` tool:

   ```bash
   git clone https://github.com/Dhanancheyan43/AdNox.git
   cd AdNox/engine
   cargo build --release -p adnox-cli
   ```

   The binary is now at `AdNox/engine/target/release/adnox` (`adnox.exe` on Windows).
   Run the commands below from the `AdNox/engine` directory.

> No Rust and don't want to install it? See **[Prebuilt binaries](#prebuilt-binaries)** below.

### Test 1 — Quick sanity check (~60 seconds)

Confirm it blocks the right things using the bundled offline list:

```bash
./target/release/adnox test doubleclick.net        # -> BLOCKED
./target/release/adnox test ads.g.doubleclick.net  # -> BLOCKED
./target/release/adnox test wikipedia.org          # -> allowed
```

Then download the full lists, run the blocker on a **test port** (5353, no admin rights needed),
and query it with real DNS:

```bash
./target/release/adnox update
./target/release/adnox run --bind 127.0.0.1:5353 --upstream 1.1.1.1:53
```

In a second terminal:

```bash
# Linux / macOS:
dig @127.0.0.1 -p 5353 doubleclick.net +short   # -> 0.0.0.0   (blocked)
dig @127.0.0.1 -p 5353 wikipedia.org   +short   # -> a real IP (allowed)

# Windows PowerShell (nslookup speaks to a custom port like this):
nslookup -port=5353 doubleclick.net 127.0.0.1   # -> 0.0.0.0
nslookup -port=5353 wikipedia.org   127.0.0.1   # -> a real address
```

The running server also prints a live `[stats] queries=… blocked=… forwarded=…` line every 30s.
Press **Ctrl-C** to stop.

### Test 2 — Actually block ads on your whole computer

This points your system's DNS at AdNox, so every app and browser is filtered.

**Windows** — open PowerShell **as Administrator**:

```powershell
cd AdNox\windows
.\install.ps1
# verify:
Resolve-DnsName doubleclick.net   # -> 0.0.0.0 (blocked)
Resolve-DnsName example.com       # -> resolves normally
```

Browse some ad-heavy sites to see the difference. To remove it and restore normal DNS:

```powershell
.\uninstall.ps1
```

**Linux / macOS** — run on the real DNS port (needs `sudo`) and point your resolver at it:

```bash
sudo ./target/release/adnox run --bind 127.0.0.1:53 --upstream 1.1.1.1:53
```

Then set your network's DNS server to `127.0.0.1`:
- **macOS:** System Settings → Network → (your connection) → Details → DNS → add `127.0.0.1`.
- **Linux:** set `nameserver 127.0.0.1` in your resolver config (e.g. via NetworkManager or
  `/etc/resolv.conf`).

Undo by removing `127.0.0.1` / switching DNS back to automatic and stopping the server (Ctrl-C).

### Test 3 — The Android app

Building the APK needs a machine with the Android toolchain (NDK + Gradle). Full copy-paste steps
are in **[android/README.md](android/README.md)**: install `cargo-ndk` and the NDK, build
`libadnox.so`, then `./gradlew assembleDebug`, `adb install` the APK, open AdNox and tap
**Turn on** (accept the one-time VPN consent prompt). Google Play disallows VPN-based blockers, so
AdNox installs as a direct APK / via F-Droid.

### Prebuilt binaries

Don't want to install Rust? The GitHub Actions CI can attach a ready-to-run `adnox` binary to each
build so you can download and run it directly — this is a planned addition (see
[ROADMAP.md](ROADMAP.md)); open an issue if you'd like it prioritized.

### Troubleshooting

- **`cargo: command not found`** — restart your terminal after installing Rust, or run
  `source "$HOME/.cargo/env"` (Linux/macOS) so `cargo` is on your `PATH`.
- **"permission denied" / can't bind port 53** — port 53 needs admin/root. Use `--bind
  127.0.0.1:5353` for a quick test (Test 1), or run with `sudo` / as Administrator (Test 2).
- **Nothing seems blocked in Test 2** — flush the DNS cache (`ipconfig /flushdns` on Windows,
  `sudo dscacheutil -flushcache` on macOS) and confirm your adapter's DNS is `127.0.0.1`.
- **Some ads still show** — v1 is DNS-level, so ads served from first-party domains aren't caught
  yet; cosmetic filtering is on the [roadmap](ROADMAP.md).

## Status

This is an early, working **Phase 1** (DNS/network blocking). Cosmetic filtering (hiding leftover
blank ad boxes), IPv6/DoT handling, scheduled list updates, and signed installers are planned —
see [ROADMAP.md](ROADMAP.md). Contributions welcome; see [CONTRIBUTING.md](CONTRIBUTING.md).

## Legal

AdNox blocks connections to ad/tracker domains for the device's owner, using publicly available,
openly-licensed filter lists. It is intended for personal privacy and security. Respect the terms
of services you use. See [LICENSE](LICENSE) (MIT).
