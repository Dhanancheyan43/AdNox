# AdNox for Windows

On Windows, AdNox runs the shared Rust DNS blocker (`adnox.exe run`) as a background task bound to
`127.0.0.1:53`, and points the system's DNS at it. Every app that resolves names through Windows
DNS is then filtered — ads/trackers are sinkholed, everything else is forwarded to your upstream
resolver (Cloudflare `1.1.1.1` by default).

## Build

Install Rust (<https://rustup.rs>) and build the release binary:

```powershell
cd engine
cargo build --release -p adnox-cli
# -> engine\target\release\adnox.exe
```

## Install (run PowerShell as Administrator)

```powershell
cd windows
.\install.ps1
```

`install.ps1`:
1. copies `adnox.exe` to `C:\Program Files\AdNox\`,
2. downloads the filter lists (`adnox update`),
3. registers a scheduled task that runs `adnox run` at boot as `SYSTEM`,
4. sets every active network adapter's DNS to `127.0.0.1`.

Verify it is working:

```powershell
Resolve-DnsName doubleclick.net   # should return 0.0.0.0
Resolve-DnsName example.com       # should resolve normally
```

## Uninstall (Administrator)

```powershell
cd windows
.\uninstall.ps1
```

This restores automatic (DHCP) DNS on all adapters, stops and removes the scheduled task, and
deletes `C:\Program Files\AdNox\`.

## Notes & roadmap

- v1 is **DNS-level** blocking. A proper Windows *service* (via the `windows-service` crate) and a
  system-tray UI with live stats are tracked in the root `ROADMAP.md`; the scheduled task is the
  simple, reliable v1 mechanism.
- Cosmetic filtering (hiding blank ad boxes) needs the HTTPS-inspection phase (local CA + proxy).
- An MSIX / Inno Setup installer that bundles all of the above into a double-click install is a
  roadmap item.
