# AdNox roadmap

Legend: ✅ done · 🚧 in progress · ⬜ planned

## Phase 0 — Foundations ✅
- ✅ Cargo workspace (`core`, `dns`, `cli`, `ffi`)
- ✅ Filter engine on Brave's `adblock` crate (ABP + hosts + plain-domain parsing)
- ✅ Subdomain-aware `DomainSet` for DNS decisions
- ✅ Unit tests + CI (`fmt`, `clippy`, `test`)

## Phase 1 — DNS blocking (MVP) ✅ / 🚧
- ✅ Local async UDP DNS server with sinkhole + upstream forwarding
- ✅ `adnox` CLI: `update`, `run`, `test`, `lists`; bundled offline seed list
- ✅ C ABI + JNI FFI bridge (`libadnox`) and C header
- ✅ Android app scaffold: `VpnService` tun loop (IPv4/UDP), JNI engine, on/off UI
- ✅ Windows install/uninstall scripts (scheduled task + system DNS)
- 🚧 End-to-end on-device verification on real Android hardware & a Windows box
- ⬜ Bind-to-port-53 privilege handling / graceful fallback messaging

## Phase 2 — Robust networking ⬜
- ⬜ IPv6 (AAAA + IPv6/UDP packets) in the Android tun loop
- ⬜ DNS-over-TCP handling
- ⬜ Block well-known DoH/DoT endpoints by IP so apps can't trivially bypass DNS filtering
- ⬜ Upstream over DoH/DoT (encrypt the user's own DNS)
- ⬜ Proper Windows service via the `windows-service` crate (replace scheduled task)

## Phase 3 — Lists & control ⬜
- ⬜ Scheduled background list updates (Android `WorkManager`, Windows task)
- ⬜ Per-list enable/disable, custom rules, per-domain allowlist (UI + persisted config)
- ⬜ Live stats surface (queries / blocked / top blocked domains) in both UIs
- ⬜ Compiled/cached engine state to speed up startup on large lists

## Phase 4 — Cosmetic filtering (opt-in) ⬜
- ⬜ Local root-CA generation + guided install
- ⬜ HTTPS-inspecting local proxy (desktop) / VPN HTTP layer (Android)
- ⬜ Inject element-hiding CSS from the lists (removes leftover blank ad boxes)
- ⬜ Clear, explicit consent UX for the certificate trade-off

## Phase 5 — Distribution & polish ⬜
- ⬜ Windows: MSIX / Inno Setup installer + system-tray app with toggle & stats
- ⬜ Android: F-Droid metadata + signed APK releases (Play Store disallows VPN blockers)
- ⬜ Auto-update mechanism for the apps
- ⬜ Battery/wakelock tuning on Android; crash reporting (opt-in)
- ⬜ Localization, accessibility pass

## Known limitations today
- DNS-level only — some ads served from first-party domains won't be blocked without cosmetic
  filtering; blank spaces may remain.
- Android loop handles IPv4/UDP DNS only.
- Google Play restricts VPN-based blockers → Android ships outside the Play Store.
