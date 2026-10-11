# AdNox architecture

## Goal

Block ads and trackers for **every app and website** on Android and Windows, from an installable
app, **without root and without a custom certificate** in v1.

## Why DNS filtering

| Mechanism | Blocks | Cosmetic (hides blank boxes) | Needs cert / root |
|-----------|--------|------------------------------|-------------------|
| Browser extension | network + cosmetic, **browser only** | yes | no |
| **DNS filtering (AdNox v1)** | requests to ad/tracker **domains**, every app | no | **no** |
| Local proxy w/ HTTPS inspection | full URLs + inject CSS | yes | **yes** (user installs CA) |

DNS filtering is the sweet spot for a first release: it covers *all* apps, needs no certificate,
and is the same mechanism AdGuard/Blokada/Pi-hole use. Cosmetic filtering is a later, opt-in phase
that layers HTTPS inspection on top (see ROADMAP).

## The shared engine (`engine/`)

A Cargo workspace. The design rule: **all filtering logic lives in Rust**, so Android and Windows
behave identically and there is one place to optimise and test.

### `adnox-core`
- Wraps Brave's [`adblock`](https://crates.io/crates/adblock) crate for full Adblock Plus URL
  matching.
- Builds a **`DomainSet`** for DNS-level decisions: a hash set with parent-domain walking so a rule
  for `doubleclick.net` also blocks `ads.g.doubleclick.net`.
- `Blocklist::from_lists()` ingests mixed input line-by-line:
  - hosts syntax (`0.0.0.0 ads.example.com`) → `DomainSet`
  - simple domain anchors (`||ads.example.com^`) → `DomainSet` *and* URL engine
  - any other ABP rule → URL engine
- Two queries: `is_blocked_domain(host)` (DNS) and `is_blocked_request(url, source, type)` (URL).

### `adnox-dns`
- An async (Tokio) UDP DNS proxy.
- `decide()` is a **pure, synchronous** step: parse the query (via `hickory-proto`), consult the
  blocklist, and for blocked names build the sinkhole response (`0.0.0.0`/`::`). It does no I/O.
- This split matters: `adblock::Engine` is `!Send`, so the engine never leaves the accept-loop
  thread. Only the upstream *forward* (which needs no engine) is spawned concurrently.
- `handle_query()` is a transport-agnostic async entry point reused by the Android VPN loop/tests.

### `adnox-ffi`
- `cdylib`/`staticlib` named `adnox`.
- **C ABI** (`adnox_engine_new`/`_free`/`_is_blocked_domain`/`_is_blocked_request`/
  `_domain_count`) — header in `engine/ffi/include/adnox.h`. Used by Windows and any native host.
- **JNI** exports (`cfg(target_os="android")`) for `com.adnox.app.NativeEngine`. Used by the app.
- Every boundary call is null-checked and wrapped in `catch_unwind`.

### `adnox-cli`
- `adnox update|run|test|lists`. Ships a **bundled seed list** (`include_str!`) so it works with
  zero downloads. On Windows this same binary *is* the DNS service (`adnox run`).

## Android (`android/`)

```
VpnService establishes a local tun, declares itself the DNS server, routes only the DNS
sentinel address through the tun.
   │
   ▼  reads IP packets
IpUtils.parseIpv4Udp ──► is it UDP:53 ?
   │ yes
   ▼
DnsPacket.questionName ──► NativeEngine.isBlockedDomain(name)   (JNI → Rust)
   │                                   │ blocked            │ allowed
   │                                   ▼                     ▼
   │                      DnsPacket.sinkhole()     forwardUpstream() via protect()ed socket
   │                                   │                     │
   └──────────── write IPv4/UDP reply back into the tun ◄────┘
```

- `protect()` keeps the upstream socket outside the VPN so forwarding does not loop.
- AdNox excludes its own package from the tunnel.
- v1 handles IPv4/UDP DNS; IPv6 & DNS-over-TCP are roadmap items.

## Windows (`windows/`)

- `adnox.exe run` binds `127.0.0.1:53`; a scheduled task starts it at boot as `SYSTEM`.
- `install.ps1` sets every adapter's DNS to `127.0.0.1`; `uninstall.ps1` reverts to DHCP.
- A proper Windows *service* (via the `windows-service` crate) and a tray UI are roadmap items.

## Data & privacy

- No AdNox servers. DNS queries go only to the user's chosen upstream resolver.
- Filter lists are fetched directly from their public sources.
- AdNox does not inspect HTTPS payloads in v1.

## Testing

- `engine/` has unit tests for domain matching, list parsing, URL blocking, sinkhole synthesis,
  the async forward path, and the C ABI round-trip. `cargo test` runs them all.
- CI (`.github/workflows/ci.yml`) runs `cargo fmt --check`, `cargo clippy`, and `cargo test`.
