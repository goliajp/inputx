# PLAN: iOS shipping — catch up to develop core

**Branch:** `feature/ios-shipping` (off `develop` @ `f69c5c3`)
**Opened:** 2026-06-10
**BACKLOG anchor:** §5 "iOS — SHELVED" → now off-shelf

## Goal

`feature/ios-shipping` merges to `develop` with the iOS InputxApp + InputxKeyboard
extension built against develop-HEAD `inputx-core`, three engines (wubi / pinyin /
nihongo) at functional parity with mac, and the pinyin minimal-debug gates
(`PINYIN_DISABLE_COMPOSE / ASSOCIATION / FUZZY = true`) propagating to iOS
exactly as they do on mac.

## Non-goals (explicit)

- Item 91 device-TBD rows (cold-start, RSS, fps) — those need Instruments on
  a real iPhone, not in scope for "catch-up"
- App Store submission flow (Items 92–105 in `ios/AppStore-checklist.md`)
- New iOS UI work (keyboard layout, settings additions, etc.)
- iOS-side L1+L2 design

## Layer audit (already done, recorded for the journal)

| Layer | State | Note |
|---|---|---|
| `core/include/inputx_core.h` FFI surface | **no drift** | All Swift call-sites in `platform/apple/Sources/InputxKit/InputxCore.swift` resolve against current header |
| Rust core `aarch64-apple-ios` build | **clean (23s)** | `libinputx_core.a` = 72 MB, all data bundled via `include_bytes!` |
| Rust core `aarch64-apple-ios-sim` build | **clean (51s)** | `aarch64-apple-ios-sim` target added to `rust-toolchain.toml` |
| iOS Swift call-sites | **already wired for 3 engines** | `KeyboardViewController` reads `engineMode` (0–3) + `japaneseEnabled`; `CandidateBar` shows `InputxCandidateSource` (.wubi / .pinyin / .japanese / .unknown) |
| iOS data resources | **fonts only** | dict / FST / KANJIDIC2 / shinjitai-backfill all linked into `libinputx_core.a` — no Resources/ refresh needed |
| `PINYIN_DISABLE_*` consts | **`pub(crate) const bool = true`** | Compile-time; iOS automatically inherits when it links the static lib |

**Implication:** the catch-up surface is entirely **build-system + smoke-test**,
not code/FFI/data. No Rust changes, no Swift changes expected.

## Work units

### WU-1 — Rust core re-baseline — ✅ DONE

- [x] `cargo build --release --target aarch64-apple-ios` (23s, clean)
- [x] `cargo build --release --target aarch64-apple-ios-sim` (51s, clean)
- [x] `rust-toolchain.toml` ← add `aarch64-apple-ios-sim`

### WU-2 — Tooling preconditions — pending

Host-level prereqs `xcodegen generate` needs before `ios/build_sim.sh` can run:

- `brew install xcodegen` (host setup; one-time)
- A simulator usable by build_sim.sh — either:
  - `xcrun simctl create sim-inputx "iPhone 15" $RUNTIME` (matches script default), or
  - set `SIMULATOR_DEVICE=<existing sim name>` env var

**Stop-and-report:** wait for user OK before installing xcodegen (host
toolchain mutation) and creating the sim (opens future GUI sessions).

### WU-3 — Sim build smoke — pending (gated on WU-2)

Run `ios/build_sim.sh`. Expected:
- xcodegen regenerates `Inputx.xcodeproj` from `ios/project.yml`
- Rust cargo build skipped as cached
- xcodebuild succeeds for both InputxApp + InputxKeyboard
- `simctl install` + `simctl launch jp.golia.inputx` complete

If xcodebuild fails: diff the failure against
`ios/Inputx.xcodeproj/project.pbxproj` (committed) — Xcode 26.5 may have
deprecated something since 2026-05-26.

### WU-4 — Behavioral parity smoke — pending (gated on WU-3)

In the simulator, with Inputx keyboard added:

| Probe | Expected | Reason |
|---|---|---|
| `pyin` / `zg` / `zongguo` / `shehv` / `hhhh` | **empty candidates** | minimal-debug gates pause COMPOSE/ASSOCIATION/FUZZY; same as mac |
| `nihao` | 你好 leads | live pinyin path |
| `gghc` (wubi) | 来 leads | wubi unchanged |
| `yngk` (wubi) | 词 / 启事 (no detour) | 繁体 sweep delivered |
| `iwa` (JapaneseOnly mode) | 巌 in candidates | JP shinjitai backfill |
| `sakai` (JapaneseOnly mode) | 堺 in candidates | 2026-06-09 sakai polish |

Any failure here = root-cause investigate, do **not** patch around.

### WU-5 — Versioning decision — pending

`ios/project.yml` has `MARKETING_VERSION: "1.3.0"`. Develop core is on the
v1.13 line. iOS and mac have evolved on different version tracks since
the v1.3.0 release split. **Open question for user:**
- Keep iOS at 1.3.0 (separate version line — independent App Store cadence)?
- Or bump to align (e.g. 1.4.0 — first iOS release on the new core baseline)?

No CHANGELOG entry until this is decided.

### WU-6 — Land — pending

After WU-4 clean:
- Commit any incidental polish (probably just `rust-toolchain.toml` + this PLAN)
- git flow `feature finish ios-shipping` (auto `--no-ff` merge to `develop`)
- Push origin develop
- Update `docs/BACKLOG.md` §5: iOS shelving lifted, ios-shipping landed
- Update memory: clear shelved-state, archive PLAN-ios-ship.md to "shipped" footer

## Stop-and-report conditions

- xcodebuild errors that aren't a trivial config tweak (Xcode 26.5 deprecation)
- sim install / launch fails
- minimal-debug paused-family probes NOT empty on iOS (would mean compile-time
  const failed to propagate — investigate, don't patch)
- Any user-OS step (System Settings keyboard add)
