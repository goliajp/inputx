# Inputx

[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![inputx-wubi](https://img.shields.io/crates/v/inputx-wubi.svg?label=inputx-wubi)](https://crates.io/crates/inputx-wubi)
[![inputx-pinyin](https://img.shields.io/crates/v/inputx-pinyin.svg?label=inputx-pinyin)](https://crates.io/crates/inputx-pinyin)
[![@goliapkg/wubi](https://img.shields.io/npm/v/@goliapkg/wubi.svg?label=%40goliapkg%2Fwubi)](https://www.npmjs.com/package/@goliapkg/wubi)
[![@goliapkg/pinyin](https://img.shields.io/npm/v/@goliapkg/pinyin.svg?label=%40goliapkg%2Fpinyin)](https://www.npmjs.com/package/@goliapkg/pinyin)

> Read this in [简体中文](README.zh-CN.md) · [日本語](README.ja.md).

A privacy-first Chinese input method (IME) for iOS, with a dual-engine
core written in Rust.

Inputx types Chinese the way 万能五笔 / 搜狗五笔 do — **Wubi 86 as the
primary engine, Pinyin as an automatic fallback** — so you never manually
switch input schemes. It runs entirely on-device: no network calls, no
analytics, no "Allow Full Access" required.

## Highlights

- **Dual engine, no manual switch** — Wubi 86 candidates first, Pinyin
  (full + 简拼 initials + partial-input prefix completion) fills in when
  Wubi yields nothing.
- **On-device learning (L0)** — three picks of the same `(input → word)`
  pair auto-pin it to the top. Learned data never leaves the device and
  is user-exportable / -importable as JSON.
- **Privacy by construction** — zero network calls, zero telemetry, no
  full keyboard access. The only data collected is whatever Apple's
  built-in crash reporter captures.
- **Perfgate-guarded keystroke** — every candidate refresh is unit-tested
  against a per-frame budget (< 16 ms on 60 Hz); input lag is treated as
  the single worst IME UX failure.
- **Rare CJK support** — optional display of Plane-2+ characters via a
  bundled cascade font (`Lab8CJKExtended`, a Plangothic subset, OFL).
- **CJK typography** — full-width / half-width toggle, ASCII→CJK
  punctuation mapping, stateful smart quotes.
- **iOS-native UX** — Dynamic Type, dark mode, VoiceOver, landscape
  reflow, per-key long-press variants, source indicators on candidates.

## Open-source engine ecosystem

The two language engines plus their WASM wrappers are independently
published to crates.io and npm. They power this IME on iOS (native) and
on the web (WASM), and are freely usable by any other Wubi / Pinyin
project under MIT or Apache-2.0:

| Component | crates.io | npm |
|---|---|---|
| Wubi 86 engine | [`inputx-wubi`](https://crates.io/crates/inputx-wubi) | — |
| Wubi 86 WASM | [`inputx-wubi-wasm`](https://crates.io/crates/inputx-wubi-wasm) | [`@goliapkg/wubi`](https://www.npmjs.com/package/@goliapkg/wubi) |
| Pinyin engine | [`inputx-pinyin`](https://crates.io/crates/inputx-pinyin) | — |
| Pinyin WASM | [`inputx-pinyin-wasm`](https://crates.io/crates/inputx-pinyin-wasm) | [`@goliapkg/pinyin`](https://www.npmjs.com/package/@goliapkg/pinyin) |

Each component has its own README in three languages and is documented
on docs.rs:

- **inputx-wubi** — [README (en)](core/crates/inputx-wubi/README.md) ·
  [简体中文](core/crates/inputx-wubi/README.zh-CN.md) ·
  [日本語](core/crates/inputx-wubi/README.ja.md) ·
  [docs.rs](https://docs.rs/inputx-wubi)
- **inputx-pinyin** — [README (en)](core/crates/inputx-pinyin/README.md) ·
  [简体中文](core/crates/inputx-pinyin/README.zh-CN.md) ·
  [日本語](core/crates/inputx-pinyin/README.ja.md) ·
  [docs.rs](https://docs.rs/inputx-pinyin)

## Architecture — core vs platform

Inputx is split into two layers so the same engine can drive every host
without a per-platform reimplementation:

```
┌─ core/                                Layer 1 — platform-agnostic Rust
│   crates/
│   ├─ inputx-wubi/                       Wubi 86 engine (pure Rust + FST)
│   ├─ inputx-wubi-wasm/                   wasm-bindgen wrapper of the engine
│   ├─ inputx-pinyin/                     Pinyin engine (pure Rust + FST)
│   ├─ inputx-pinyin-wasm/                 wasm-bindgen wrapper of the engine
│   ├─ inputx-core/                       Composite (dual-engine router,
│   │                                     L0 user-learning, locale, session)
│   └─ inputx-core-ffi/                   C ABI (extern "C" + cbindgen) →
│                                         libinputx_core.{a,dylib}
│
├─ platform/                            Layer 2 — host-specific glue
│   └─ apple/                              Shared Apple Swift package
│       └─ Sources/InputxKit/                FFI wrapper + L0 + locale + settings
│
├─ ios/                                  iOS app + Keyboard extension
│                                        (consumes platform/apple via SPM)
├─ mac/                                  macOS IME (IMK + NSStatusItem)
│                                        (consumes platform/apple via SPM)
└─ build/, data/, tools/                  Build artifacts, raw data sources
```

The split makes the architecture explicit:

- **Layer 1 (core)** is pure Rust. No `extern "C"`, no `wasm-bindgen`, no
  Apple imports — the composite engine is reusable from any Rust context
  (CLI testbed, server-side IME service, future native consumers).
- **Layer 1 sidecars (`inputx-core-ffi`, `inputx-*-wasm`)** wrap the pure
  engine for the two binding shapes the world cares about: C ABI for
  native hosts, wasm-bindgen for browsers / Node.
- **Layer 2 (platform)** is host-specific glue around the FFI. Today
  that's `platform/apple/` (a Swift package consumed by both `ios/` and
  `mac/`); future hosts each get their own platform sub-tree (`web/`
  for JS / TypeScript, `linux/` for IBus/Fcitx5, `windows/` for TSF).

Each layer can evolve independently as long as the FFI surface
(`core/include/inputx_core.h`, auto-generated by cbindgen) stays stable.

`inputx-core` and `inputx-core-ffi` are **not** published to crates.io —
they're app-internal. The four published crates listed below give
external consumers a clean entry point to either the Wubi engine or the
Pinyin engine on their own.

## Build

Requires Rust ≥ 1.85 (edition 2024), Xcode 16+, and
[`xcodegen`](https://github.com/yonaskolb/XcodeGen).

```sh
# Rust core — runs the full test suite (253 tests + perfgate, 5 crates).
cd core && cargo test --release

# Bench (criterion) — pinyin and composite hot paths.
cd core && cargo bench -p inputx-pinyin
cd core && cargo bench -p inputx-core

# iOS simulator build.
cd ios && ./build_sim.sh

# On-device build (requires provisioning profile + signing identity).
cd ios && ./build_device.sh
```

The C header (`core/include/inputx_core.h`) is generated by cbindgen
during `cargo build` and is `.gitignore`d, so a fresh clone must build
the Rust core before the iOS target can link. The build scripts handle
this ordering for you.

## Status

| Surface | State |
|---|---|
| iOS keyboard extension + container app | v1 implementation complete; on-device verified; pending TestFlight / App Store |
| macOS IME (InputMethodKit) | v1.0.0 feature-parity with iOS — candidate panel, mode picker, CJK locale, L0 persistence, menubar settings. `mac/release.sh` ready for notarization once a Developer ID cert is provisioned |
| Web (browser / Node) | Engine wasm crates published (`@goliapkg/wubi`, `@goliapkg/pinyin`); composite layer not yet exposed as `inputx-core-wasm` — slot reserved in the architecture |
| Linux (IBus / Fcitx5) | Not yet started. FFI surface ready. |
| Windows (TSF) | Not yet started. FFI surface ready. |

Out of scope for v1: iCloud L0 sync, ja-JP / ko-KR localization.

## Porting to other platforms

The C ABI (`core/include/inputx_core.h`, auto-generated by cbindgen) is
the contract every native host implements against. To bring up a new
platform:

1. **Build the right Rust target.** `cargo build --release -p inputx-core-ffi --target <triple>` produces `libinputx_core.{a,dylib,so,dll}`. The header lives at `core/include/inputx_core.h`.
2. **Write a thin host glue layer** in `platform/<host>/`. The Apple one (`platform/apple/Sources/InputxKit/`) is the reference: ~500 lines wrapping the FFI in idiomatic types, plus a file-persistence helper and a locale helper. Mirror the same surface in your host language.
3. **Wire the host into its native IME framework.** macOS uses InputMethodKit; iOS uses `UIInputViewController`; Linux has IBus + Fcitx5 plugin APIs; Windows uses TSF (Text Services Framework). Your host glue calls into InputxKit-equivalent code from the framework's keyboard event hook + draws the candidate panel using the framework's primitives.

For web hosts the path is shorter — the two engine WASM packages
(`@goliapkg/wubi`, `@goliapkg/pinyin`) are already on npm and can drive a
JS IME directly. A future `inputx-core-wasm` will expose the composite
router + L0 persistence layer to JS in one bundle.

## License

Dual-licensed under **MIT** ([`LICENSE-MIT`](LICENSE-MIT)) **OR
Apache-2.0** ([`LICENSE-APACHE`](LICENSE-APACHE)) © 2026 GOLIA K.K., at
your option.

Bundled third-party data and resources retain their own licenses — see
the in-app Acknowledgements screen and `core/crates/inputx-pinyin/LICENSE-*`
for the full attribution chain (Unihan, jieba, pypinyin, Leipzig,
SUBTLEX-CH, Plangothic / OFL).
