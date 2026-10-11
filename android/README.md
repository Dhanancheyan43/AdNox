# AdNox for Android

A no-root ad/tracker blocker built on Android's `VpnService`. A local-only VPN captures DNS
queries from every app and filters them through the shared Rust engine (`libadnox.so`); blocked
domains are sinkholed to `0.0.0.0`, everything else is forwarded to your upstream resolver.

> No traffic leaves your device through AdNox — the "VPN" is local-only and exists purely so
> Android routes DNS through the app. AdNox does not (and in this version cannot) read the
> contents of your HTTPS traffic.

## What's here

```
android/
  app/src/main/
    AndroidManifest.xml
    assets/adnox-seed.txt                 bundled offline blocklist
    java/com/adnox/app/
      MainActivity.kt                     on/off UI + "update lists"
      AdNoxVpnService.kt                  the VpnService tun loop
      NativeEngine.kt                     JNI wrapper over libadnox.so
      DnsPacket.kt                        DNS question parse + sinkhole synthesis
      IpUtils.kt                          IPv4/UDP parse + build + checksums
      FilterLists.kt                      list storage + download
```

## Building

### 1. Build the native engine (`libadnox.so`)

Install the Rust Android targets and [`cargo-ndk`](https://github.com/bbqsrc/cargo-ndk), then
cross-compile the FFI crate for each ABI straight into the app's `jniLibs`:

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
cargo install cargo-ndk

cd engine
cargo ndk \
  -t arm64-v8a -t armeabi-v7a -t x86_64 \
  -o ../android/app/src/main/jniLibs \
  build --release -p adnox-ffi
```

This produces:

```
android/app/src/main/jniLibs/arm64-v8a/libadnox.so
android/app/src/main/jniLibs/armeabi-v7a/libadnox.so
android/app/src/main/jniLibs/x86_64/libadnox.so
```

(`ANDROID_NDK_HOME` must point at an installed NDK, e.g. via Android Studio's SDK Manager.)

### 2. Build the APK

```bash
cd android
./gradlew assembleDebug        # app/build/outputs/apk/debug/app-debug.apk
# or a signed release:
./gradlew assembleRelease
```

Install with `adb install app-debug.apk`, open AdNox, tap **Turn on**, and accept the one-time
Android VPN consent dialog.

## Distribution note

Google Play restricts `VpnService`-based content blockers, so AdNox for Android is distributed as
a **direct APK** / via **F-Droid**, not the Play Store. See the root `ROADMAP.md`.

## Current scope & limitations (v1)

- Filters **IPv4 UDP DNS**. IPv6 DNS and DNS-over-TCP are on the roadmap.
- DNS-level blocking only (no cosmetic/element hiding yet — that needs the HTTPS-inspection phase).
- Apps using hard-coded DNS-over-HTTPS/TLS (e.g. to `8.8.8.8`) can bypass DNS filtering; blocking
  those endpoints by IP is a roadmap item.
