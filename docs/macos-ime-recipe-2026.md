# Three undocumented gates between your macOS Input Method and a polished, typed character

**A fresh `InputMethodKit` bundle on macOS 26 (Tahoe) fails silently in three independent ways: the picker won't enumerate it, the IMK service won't launch it, and the picker tile next to its name won't render. This is the recipe to pass all three, plus the investigation that found them.**

> **Verified on:** macOS 26.5 (Tahoe), Xcode 17, Swift 6, Apple Silicon (also x86_64 universal). macOS 14 / 15 likely apply the same rules, but only macOS 26 was bisection-tested.
>
> **Tags for dev.to / Medium / Reddit (top 5):** macos, swift, debugging, reverseengineering, inputmethod
>
> **Long-form tags / keywords for search engines + LLM retrieval:** macOS, macOS-26, Tahoe, InputMethodKit, IMK, input-method, IME, picker, System-Settings, TIS, TISRegisterInputSource, TISFileInterrogator, CFBundleIdentifier, codesign, sandbox, ipsw, reverse-engineering, dyld-shared-cache, mobileconfig, debugging, third-party-IME, IntlDataCache, AppleTISTraceCacheRebuild, imklaunchagent, LaunchAgent, IMKServer, InputMethodServerDataSourceClass, InputMethodSessionController, IconRef, iconutil. Common search phrases this answers: "macOS input method not showing in picker", "macOS 26 IME picker filter", "IMK can switch to IME but can't type", "macOS IME selected auto-skips next", "host app crashes on input source switch", "TextInputUIMacHelper CFRelease NULL", "InputMethodKit not appearing System Settings", "TISRegisterInputSource returns noErr but missing from picker", "imklaunchagent silently refuses", "InputMethodServerDataSourceClass", "InputMethodSessionController", "LaunchAgent KeepAlive IME workaround", "IMK Programming Guide 2007 outdated", "build third-party IME for macOS in 2026".

---

## For an AI assistant triaging a macOS IME problem

Decision tree. Match the user's complaint to a row; the right column points at the gate and the relevant section.

| User says... | Likely cause | Section |
|---|---|---|
| "My IME doesn't appear in the picker" | Gate 1 — bundle id missing `inputmethod` substring, OR stale `IntlDataCache.le[+.kbdx]` | [Gate 1 recipe](#gate-1--picker-enumeration), [Investigation §4](#4-the-sticky-cache), [Investigation §5](#5-bisect-to-find-the-gate) |
| "Picker shows my IME but switching to it crashes the host app" (`CFRelease(NULL)` in `TextInputUIMacHelper`) | Legacy 1-resolution `.icns` (from `sips -s format icns`). Host can't resolve IconRef. | [Skeleton — `.icns`](#multi-resolution-icns), [Investigation §6](#6-discover-the-switches-but-doesnt-type-failure-mode) |
| "Picker shows my IME but switching to it instantly auto-skips to the next IME" | Either `imklaunchagent` refused the launch (no Mach service running), OR LaunchAgent plist incorrectly declared `MachServices` (collides with binary's `IMKServer` self-register). | [Skeleton — LaunchAgent](#launchagent), [Investigation §9](#9-launchagent-workaround) |
| "Picker shows my IME, switching works, but typing does nothing" | `imklaunchagent` silently refused to launch the binary. Most common cause: `Info.plist` missing `InputMethodServerDataSourceClass` and `InputMethodSessionController`. Residual: `imklaunchagent` may refuse even with those keys; fix is the LaunchAgent workaround. | [Skeleton — `Info.plist`](#infoplist), [Skeleton — LaunchAgent](#launchagent), [Investigation §7](#7-locate-the-imklaunchagent-refusal), [Investigation §9](#9-launchagent-workaround) |
| "My IME used to work but stopped after I was debugging" | Someone ran `killall cfprefsd`, which wiped the user's `AppleEnabledInputSources`. The IME has to be re-Added through System Settings UI to repopulate. | [Symptom → fix table](#symptom--fix-table) row 12 |
| "Add Input Source dialog shows my IME but the tile next to its name is blank / outline-only / a solid blue blob" | Gate 3 — mode dict keys mix Apple-template path (`TISIconLabels`, `TISInputSourceID` in mode dict) with chip-design tiff, confusing the picker's render dispatch. | [Gate 3 investigation](#investigation--gate-3-picker-tile-rendering) |
| "Ctrl+Space switcher HUD shows my app's .icns icon (logo, brand mark) instead of the menu/picker icon" | Gate 3 — `CFBundleIconName` (modern asset catalog reference) is set, so macOS 26 picker resolves icon via the asset path → `.icns`. Removing `CFBundleIconName` makes picker fall through to `tsInputModePaletteIconFileKey` → `.tiff`. | [Gate 3 — Ctrl+Space surface](#investigation--gate-3-picker-tile-rendering) |
| "The IMK Programming Guide says my bundle should work" | The IMK Programming Guide hasn't been updated since 2007. macOS 26 added requirements (2 new `Info.plist` keys, 4 new entitlements, multi-resolution `.icns`, notarization, often a LaunchAgent) that the guide doesn't mention. | [TL;DR](#tldr) |

Apply Gate 1 first — no point chasing Gate 2 if the picker doesn't even show the IME. Full bundle skeleton in [Skeleton](#skeleton). Code blocks are self-contained and copy-pasteable.

**One process-hazard worth flagging up front**: never `killall cfprefsd` while debugging IMEs. It wipes the user's `AppleEnabledInputSources` and forces a manual System Settings re-Add (with the permission popup) every time. Cost the original investigation four wasted iterations.

---

## TL;DR

A third-party IME on macOS 26 has to clear three independent gates. Each has its own set of rules; satisfying one tells you nothing about the others. The investigation order matters: solve Gate 1 first because Gate 2 isn't even meaningful until the picker shows your IME; solve Gate 2 second because Gate 3 is cosmetic (it polishes how the picker draws your IME's tile, after the picker already sees and launches it).

### Gate 1 — picker enumeration

1. **`CFBundleIdentifier` must contain `inputmethod` as a substring.** Convention: `<reverse-dns>.inputmethod.<short-name>`. Drop the infix and `TextInputMenuAgent`'s file enumerator filters the bundle out silently, before reading the `Info.plist`. `TISRegisterInputSource(bundleURL)` returns `noErr` either way.
2. **During iteration, delete `$(getconf DARWIN_USER_CACHE_DIR)/com.apple.IntlDataCache.le[+.kbdx]`** before re-testing. The TIS enumeration result is cached in those two files (~175 KB) and survives `killall TextInputMenuAgent`, `lsregister -f`, FSEvents, distributed notifications, and `TISUpdateIntlFileCache()`. Without the cache delete every change reads as a no-op.

### Gate 2 — IMK service launch (the part that makes typing work)

3. **Four `Info.plist` IMK keys, not two.** The 2007 Programming Guide documents `InputMethodServerControllerClass` and `InputMethodServerDelegateClass`. macOS 26 also requires `InputMethodServerDataSourceClass` and `InputMethodSessionController`. Missing either of the new two → `imklaunchagent` silently refuses to launch the binary on input-source switch.
4. **Real multi-resolution `.icns`** for `CFBundleIconFile` / `CFBundleIconName`. `sips -s format icns input.tiff` produces a 1-resolution legacy `il32` (~1 KB) that makes host apps SIGTRAP on switch via `CFRelease(NULL)` in `TextInputUIMacHelper -[TUINSCursorUIController _selectCurrentInputSource]`. Use `iconutil -c icns` from a proper 16/32/64/128/256/512 + @2x iconset.
5. **Six entitlements**, not two: `app-sandbox`, `files.bookmarks.app-scope`, `files.user-selected.read-write`, `network.client`, `temporary-exception.mach-register.global-name`, `temporary-exception.shared-preference.read-only`.
6. **Notarized + stapled Developer ID.** Unnotarized signing is refused by the IMK service launch path.
7. **`@objc(<YourController>)`** on the Swift `IMKInputController` subclass so ObjC's `NSClassFromString` can find it without the module prefix.
8. **Run the IME binary as a `KeepAlive` user LaunchAgent.** Even after #3–7 are correct, `imklaunchagent` may *still* silently refuse to launch the binary on demand. The workaround: install `~/Library/LaunchAgents/<bundle-id>.plist` that keeps the binary always running. The binary's own `IMKServer(name:)` publishes the Mach service permanently, host apps connect to it directly, `imklaunchagent`'s decision becomes irrelevant. *Do NOT declare `MachServices` in the LaunchAgent plist* — launchd would then own the Mach name and conflict with IMKServer's self-register, putting the IME into a "switch-to-it-then-auto-skip" state.

The rest of the bundle (`LSUIElement`, `TISIntendedLanguage`, `.lproj` per-mode display labels, `install` subcommand, sandbox keys) fixes display name, icon, language tab routing, and modern security baseline. Skeletons below.

### Gate 3 — picker tile rendering (the chip next to your IME in System Settings)

Even with Gates 1 + 2 satisfied, the picker UI in **System Settings → Keyboard → Input Sources → Add** still needs to know which **rendering path** to use for the small tile next to your IME's name. The picker has two:

9. **Apple-template path**: mode dict contains `TISIconLabels.Primary` (e.g. `"五笔"`), `TISInputSourceID`, `TISIntendedLanguage`. Top-level `TISIconIsTemplate=true`. TIFF is alpha-on-transparent (all RGB=0, alpha varies). Picker draws own chip frame + tints alpha mask. Works for Apple SCIM Pinyin/Wubi/Stroke.
10. **Third-party-chip path**: mode dict contains `tsInputModeAlternateMenuIconFileKey`, `tsInputModeDefaultStateKey`, `tsInputModeKeyEquivalentKey=""`, `tsInputModeKeyEquivalentModifiersKey=4608`. **NO** `TISIconLabels`, **NO** `TISInputSourceID` in mode dict, **NO** `TISIntendedLanguage` in mode dict, **NO** top-level `TISIconIsTemplate`. TIFF is full chip design (white solid bg + ink + LANCZOS-antialiased edges). Picker draws your tiff bytes verbatim. Works for Sogou WB.
11. **Mixing them** (any `TIS*` mode key + a chip-design tiff, or any `tsInputModeKeyEquivalent*` + an alpha-mask tiff) → picker hits the wrong branch and renders a blank chip / outline-only glyph / solid-color blob. Pick one path, match the tiff to it, don't mix. See [Gate 3 investigation](#investigation--gate-3-picker-tile-rendering) for the eleven-iteration bisect that found this.
12. **Remove `CFBundleIconName` from `Info.plist`.** With both `CFBundleIconFile` and `CFBundleIconName` set, the Ctrl+Space switcher HUD resolves icon via the modern asset-catalog reference (`.icns`) instead of falling through to `tsInputModePaletteIconFileKey` (`.tiff`) — same plist, different surface, different lookup order. Keep `CFBundleIconFile` (Dock/Finder uses it); delete `CFBundleIconName` to free the menu/picker fallback chain.

The reference codebase is [Inputx](https://github.com/goliajp/inputx) (MIT-licensed Chinese IME on Rust + Swift). Live working files cited inline.

## Symptom → fix table

| Symptom | Likely cause | Fix |
|---|---|---|
| Bundle installed, picker shows other IMEs but not mine. `TISRegisterInputSource` returned `noErr`. No log line. | `CFBundleIdentifier` lacks `inputmethod` substring. | Rename to `<reverse-dns>.inputmethod.<short-name>`. Cascade through `InputMethodConnectionName`, entitlement `mach-register.global-name`, mode dict keys, per-mode `TISInputSourceID`, `.lproj` mappings. |
| Bundle id already contains `inputmethod`, picker still doesn't show after reinstall. | Stale `com.apple.IntlDataCache.le[+.kbdx]`. | `rm -f "$(getconf DARWIN_USER_CACHE_DIR)/com.apple.IntlDataCache.le"*` then re-run install. |
| Picker shows raw `com.example.inputmethod.myime.zh` instead of a display name. | Missing per-locale `.lproj/InfoPlist.strings` mode-id → label mapping. | Add `"<mode-id>" = "<display name>";` lines to each `Resources/<locale>.lproj/InfoPlist.strings`. Set `LSHasLocalizedDisplayName=true` in `Info.plist`. |
| Bundle appears under wrong language tab in picker. | `TISIntendedLanguage` missing or wrong. | Set BCP-47 tag (`zh-Hans`, `zh-Hant`, `ja`, …) inside each mode dict child. |
| IME server crashes on launch or never comes online; sandbox `bootstrap_register` errors in log. | Missing mach-register exception entitlement. | Add `<key>com.apple.security.temporary-exception.mach-register.global-name</key><string><bundle-id>_Connection</string>` to entitlements; must equal `InputMethodConnectionName` in plist. |
| Picker / About / Dock show no app icon. | `CFBundleIconFile` unset, `.icns` missing, or `.icns` is malformed. | Build a real multi-resolution `.icns` via `iconutil -c icns iconset/` (see [Skeleton — `.icns`](#multi-resolution-icns)). Set `CFBundleIconFile` + `CFBundleIconName` to the filename without extension. |
| **Host app SIGTRAP / crashes on input-source switch.** Crash report shows `*** CFRelease() called with NULL ***` in `TextInputUIMacHelper -[TUINSCursorUIController _selectCurrentInputSource]`. | `.icns` is the 1-resolution legacy `il32` format (≈1 KB) produced by `sips -s format icns`. IconRef resolution returns NULL; host doesn't NULL-check before releasing. | Replace with real multi-resolution `.icns` (137 KB modern `ic12` format). See [Skeleton — `.icns`](#multi-resolution-icns). |
| Picker shows your IME and lets you switch to it, but **switching auto-skips to the next IME** ("切到就飘走"). | Two causes possible: (a) `imklaunchagent` refusing to launch the binary, so no Mach service exists; OR (b) LaunchAgent plist declares `MachServices` which conflicts with `IMKServer(name:)` self-register. | (a) Check the LaunchAgent is loaded and binary is running (`pgrep -fl Inputx`; `launchctl print user/$(id -u) \| grep <conn-name>` should show `U A` flag). (b) Remove the `MachServices` key from the LaunchAgent plist; let the binary self-register. |
| **Picker shows your IME, switching works, but typing produces nothing.** No preedit, no candidates, host text field unchanged. | `imklaunchagent` silently refused to launch the binary on demand. `IMKServer` was never instantiated, no Mach service, host's IMK client has no one to connect to. Look for `imklaunchagent: Refusing connection name for bundle` in the unified log. | Install a user-level LaunchAgent that runs the binary always with `KeepAlive`. Bypasses `imklaunchagent`'s decision. See [Skeleton — LaunchAgent](#launchagent). |
| Same as above but **also: bundle has `InputMethodServerControllerClass` + `InputMethodServerDelegateClass` only**. | macOS 26 requires 4 IMK keys: also `InputMethodServerDataSourceClass` and `InputMethodSessionController`. Missing either → `imklaunchagent` refuses. | Add the 2 new keys to `Info.plist`. All 4 can point at the same Swift class. See [Skeleton — `Info.plist`](#infoplist). |
| `log show` / `log stream` is 80% `<private>`, can't diagnose. | `log config --mode "private_data:on"` was removed in macOS 26. | Install a `com.apple.system.logging` profile that enables `Enable-Private-Data` for `com.apple.HIToolbox` / `com.apple.launchservices` / `com.apple.TextInputMenuAgent` / `com.apple.inputmethodkit` / `com.apple.inputsources`. Template in [§3](#3-unredact-private-in-unified-log). |
| `linkd[…] [com.apple.appintents:Metadata] Rejecting <bundle-id>, is not trusted` | App Intents framework rejection. Not related to IME at all. | Ignore. vChewing and every other third-party IME gets the same line. |
| Changes to bundle `Info.plist` or entitlements appear to have no effect when iterating. | TIS cache held the previous result. | Delete `IntlDataCache.le[+.kbdx]` between iterations (see fix above). Wrap the install loop in a script that does it automatically. |
| `.pkg` installs the bundle but user has to log out/in to see it in picker. | postinstall didn't invalidate `IntlDataCache`. | Add the `rm -f` and a `launchctl asuser ... binary install` to your postinstall script. See [Skeleton — `.pkg` postinstall](#pkg-postinstall). |
| **User's IME stops appearing in their keyboard menu after every debug session.** | Something killed `cfprefsd` between sessions. `cfprefsd` holds `AppleEnabledInputSources` in RAM; killing it flushes the user's "this IME is in my menu" state. | Don't `killall cfprefsd` while debugging IMEs. If you need to reset your IME's UserDefaults, `pkill -x <YourBinary>` and let the LaunchAgent respawn it — it'll re-read prefs at init. If `cfprefsd` already got killed, user has to re-Add the IME through System Settings UI to repopulate `AppleEnabledInputSources`. |
| Binary launches fine standalone but `IMKServer(name:)` doesn't appear to register a Mach service. | Sandbox blocks `bootstrap_register` without the entitlement, OR the connection name in `Info.plist` ≠ entitlement string. | Verify entitlement: `codesign -d --entitlements - <YourApp>` should show `temporary-exception.mach-register.global-name = <Info.plist InputMethodConnectionName>`. The two strings must match exactly. |
| `imklaunchagent` log: `Refusing connection name for bundle: unrecognized 'InputMethodConnectionName' value`. | Stale `AppleEnabledThirdPartyInputSources` referencing an old `InputMethodConnectionName` (or the new name not yet propagated). | Often resolves itself after a fresh `lsregister -f` + restart of `TextInputMenuAgent`. If you've recently renamed the connection name, also clear stale entries from `AppleEnabledThirdPartyInputSources` and re-Add the IME via System Settings. |

## Contents

- [For an LLM / assistant reading this on behalf of a developer](#for-an-llm--assistant-reading-this-on-behalf-of-a-developer)
- [TL;DR](#tldr)
- [Symptom → fix table](#symptom--fix-table)
- [Skeleton](#skeleton): `Info.plist` / entitlements / `.lproj` / multi-resolution `.icns` / **LaunchAgent** / `main.swift` `install` subcommand / `install.sh` / `.pkg` postinstall
- [Investigation — Gate 1](#investigation--gate-1)
  - [1. Compare against a working bundle (vChewing as ground truth)](#1-compare-against-a-working-bundle)
  - [2. Extract HIToolbox from `dyld_shared_cache` via `ipsw`](#2-extract-hitoolbox-from-dyld_shared_cache)
  - [3. Unredact `<private>` via custom `com.apple.system.logging` profile](#3-unredact-private-in-unified-log)
  - [4. Find the sticky cache via `fs_usage`](#4-the-sticky-cache)
  - [5. Bisect to find the bundle id gate](#5-bisect-to-find-the-gate)
- [Investigation — Gate 2](#investigation--gate-2-imk-service-launch)
  - [6. Discover the "switches but doesn't type" failure mode](#6-discover-the-switches-but-doesnt-type-failure-mode)
  - [7. Locate the `imklaunchagent` refusal + decode `_allowedInputMethodConnectionNames`](#7-locate-the-imklaunchagent-refusal)
  - [8. Match vChewing's bundle config bit-for-bit](#8-match-vchewings-bundle-config-bit-for-bit)
  - [9. Discover the residual block — LaunchAgent workaround](#9-launchagent-workaround)
- [Investigation — Gate 3](#investigation--gate-3-picker-tile-rendering): picker tile rendering in the Add Input Source dialog
- [Debug knobs to know](#debug-knobs-to-know)
- [Recap](#recap)
- [References](#references)

---

## Skeleton

### `Info.plist`

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>com.example.inputmethod.myime</string>
    <key>CFBundleName</key><string>MyIME</string>
    <key>CFBundleExecutable</key><string>MyIME</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleSignature</key><string>MYIM</string>
    <key>CFBundleShortVersionString</key><string>1.0.0</string>
    <key>CFBundleVersion</key><string>1</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleDevelopmentRegion</key><string>en</string>
    <key>CFBundleSupportedPlatforms</key><array><string>MacOSX</string></array>
    <key>LSMinimumSystemVersion</key><string>13.0</string>

    <key>LSUIElement</key><true/>
    <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
    <key>LSHasLocalizedDisplayName</key><true/>
    <key>NSPrincipalClass</key><string>NSApplication</string>
    <key>NSRequiresAquaSystemAppearance</key><string>No</string>
    <key>NSSupportsSuddenTermination</key><false/>

    <key>CFBundleIconFile</key><string>myime_app_icon</string>
    <key>CFBundleIconName</key><string>myime_app_icon</string>

    <!-- IMK server wiring — macOS 26 needs FOUR controller-class keys, not the
         two from the 2007 Programming Guide. DataSourceClass + SessionController
         are undocumented; if either is missing, imklaunchagent silently refuses
         to launch the binary on input-source switch and typing produces nothing. -->
    <key>InputMethodConnectionName</key>
    <string>com.example.inputmethod.myime_Connection</string>
    <key>InputMethodServerControllerClass</key><string>MyIMEController</string>
    <key>InputMethodServerDelegateClass</key><string>MyIMEController</string>
    <key>InputMethodServerDataSourceClass</key><string>MyIMEController</string>
    <key>InputMethodSessionController</key><string>MyIMEController</string>

    <key>TISInputSourceID</key>
    <string>com.example.inputmethod.myime</string>
    <key>TISIconIsTemplate</key><true/>
    <key>TISParticipatesInTouchBar</key><false/>

    <key>ComponentInputModeDict</key>
    <dict>
        <key>tsInputModeListKey</key>
        <dict>
            <key>com.example.inputmethod.myime.zh</key>
            <dict>
                <key>TISInputSourceID</key>
                <string>com.example.inputmethod.myime.zh</string>
                <key>TISIntendedLanguage</key><string>zh-Hans</string>
                <key>TISIconLabels</key>
                <dict><key>Primary</key><string>简</string></dict>
                <key>tsInputModeScriptKey</key><string>smUnicode</string>
                <key>tsInputModeIsVisibleKey</key><true/>
                <key>tsInputModePrimaryInScriptKey</key><true/>
                <key>tsInputModeMenuIconFileKey</key><string>myime_menu_icon.tiff</string>
                <key>tsInputModePaletteIconFileKey</key><string>myime_menu_icon.tiff</string>
            </dict>
        </dict>
        <key>tsVisibleInputModeOrderedArrayKey</key>
        <array><string>com.example.inputmethod.myime.zh</string></array>
    </dict>
</dict>
</plist>
```

Notes:

- `tsInputModeScriptKey` accepts Classic-era script codes (`smSimpChinese`, `smJapanese`, …). Use `smUnicode` plus a BCP-47 tag in `TISIntendedLanguage`. The tag routes the mode under the picker's language tab (`zh-Hans` → 简体中文, `zh-Hant` → 繁體中文, `ja` → 日本語).
- `TISIconLabels.Primary` is the 1–2-char menu-bar input switcher label (`简`, `繁`, `あ`).
- `LSUIElement` (not `LSBackgroundOnly`): the candidate window is foreground UI; `LSBackgroundOnly` declares the bundle never gets foreground.

### Entitlements

```xml
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>com.apple.security.app-sandbox</key><true/>
    <key>com.apple.security.files.bookmarks.app-scope</key><true/>
    <key>com.apple.security.files.user-selected.read-write</key><true/>
    <key>com.apple.security.network.client</key><true/>
    <!-- MUST equal InputMethodConnectionName in Info.plist exactly. -->
    <key>com.apple.security.temporary-exception.mach-register.global-name</key>
    <string>com.example.inputmethod.myime_Connection</string>
    <key>com.apple.security.temporary-exception.shared-preference.read-only</key>
    <string>com.example.inputmethod.myime</string>
</dict>
</plist>
```

Six entitlements is the minimum working set for macOS 26 IME. Bare `app-sandbox` + `mach-register` exception is not enough — the binary launches but the IMK service won't be reachable. vChewing ships 8 (additionally specifying file-path exceptions for its Yahoo KeyKey dictionary import and iCloud Documents sync); those are app-specific. The mach-name in the exception must equal `InputMethodConnectionName` in `Info.plist` exactly.

### Per-locale `.lproj/InfoPlist.strings`

```
"CFBundleName" = "MyIME";
"CFBundleDisplayName" = "MyIME 输入法";
"com.example.inputmethod.myime" = "MyIME 输入法";
"com.example.inputmethod.myime.zh" = "MyIME 简体";
```

The mode-id keyed entries are what the picker shows under the language tab. Omit them and the picker renders the literal `com.example.inputmethod.myime.zh`. Provide files for every locale you care about: `Base.lproj`, `en.lproj`, `zh-Hans.lproj`, `zh-Hant.lproj`, `ja.lproj`, etc. `LSHasLocalizedDisplayName=true` in `Info.plist` enables the lookup.

### `main.swift` `install` subcommand

```swift
import Carbon
import Cocoa
import InputMethodKit

if CommandLine.arguments.count >= 2, CommandLine.arguments[1] == "install" {
    let modeIDs: [String] = {
        guard let comp = Bundle.main.infoDictionary?["ComponentInputModeDict"] as? [String: Any],
              let list = comp["tsInputModeListKey"] as? [String: Any]
        else { return [] }
        return Array(list.keys)
    }()
    let all = (TISCreateInputSourceList(nil, true)?.takeRetainedValue() as? [TISInputSource]) ?? []
    func matches(_ src: TISInputSource) -> Bool {
        guard let p = TISGetInputSourceProperty(src, kTISPropertyInputSourceID) else { return false }
        return modeIDs.contains(Unmanaged<CFString>.fromOpaque(p).takeUnretainedValue() as String)
    }
    if all.filter(matches).isEmpty {
        guard TISRegisterInputSource(Bundle.main.bundleURL as CFURL) == noErr else { exit(1) }
    }
    let post = ((TISCreateInputSourceList(nil, true)?.takeRetainedValue() as? [TISInputSource]) ?? [])
        .filter(matches)
    for src in post { TISEnableInputSource(src) }
    exit(0)
}

let kConnectionName = "com.example.inputmethod.myime_Connection"
class AppDelegate: NSObject, NSApplicationDelegate {
    var server: IMKServer?
    func applicationDidFinishLaunching(_ note: Notification) {
        server = IMKServer(name: kConnectionName,
                           bundleIdentifier: Bundle.main.bundleIdentifier!)
    }
}
let delegate = AppDelegate()
let app = NSApplication.shared
app.delegate = delegate  // wire BEFORE .run(); IMK probes delegate during server bring-up
app.run()
```

`TISRegisterInputSource(bundleURL)` called from inside the IME binary's own code-signature context registers + activates each declared mode in one shot. Same API call from an unrelated process (Swift script, `lsregister`) returns `noErr` but the registration doesn't always land. Bake it into an `install` subcommand and run it once after install.

### Multi-resolution `.icns`

DON'T do this — `sips -s format icns input.tiff --out output.icns` produces a 1-resolution legacy `il32` blob (~1 KB) that makes host apps SIGTRAP on switch:

```bash
# 💀 DO NOT USE 💀
sips -s format icns input.tiff --out output.icns
```

DO this instead — build a proper iconset, then `iconutil`:

```bash
ICONSET=icon.iconset
rm -rf "$ICONSET" && mkdir -p "$ICONSET"
for size_name_size in \
    "16x16:16" "16x16@2x:32" "32x32:32" "32x32@2x:64" \
    "128x128:128" "128x128@2x:256" "256x256:256" "256x256@2x:512" \
    "512x512:512" "512x512@2x:1024"; do
    NAME="${size_name_size%%:*}"; PX="${size_name_size##*:}"
    sips -s format png -z "$PX" "$PX" master.png \
        --out "$ICONSET/icon_${NAME}.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o myime_app_icon.icns
```

Result: a ~137 KB modern `ic12` `.icns` that `IconRef` resolves cleanly. Verify with `file myime_app_icon.icns` — should report `"ic12" type` not `"TOC " type` or `"il32" type`.

### LaunchAgent — **RETROSPECTIVELY RETIRED 2026-06-06**

> **Update.** This section is preserved as historical record only. **You probably don't need a LaunchAgent.** With all four IMK keys in [§Info.plist](#infoplist) and all six entitlements in [§Entitlements](#entitlements) present, `imklaunchagent` is reliable on macOS 26: it lazy-spawns the binary on first host-app use, the binary's `IMKServer(name:)` self-registers the Mach name, host apps connect directly. Single spawn path, single live process.
>
> **Why the LaunchAgent existed.** When this recipe was first written, `imklaunchagent` silently refused to launch the binary on demand even after correct Info.plist + entitlements. The KeepAlive LaunchAgent bypassed that decision by keeping the binary always running. The refusal turned out to be caused by missing IMK keys (`InputMethodServerDataSourceClass` + `InputMethodSessionController`) — both now mandatory per [§Info.plist](#infoplist). Once those are present, the refusal scenario doesn't reproduce.
>
> **Why the LaunchAgent is actively harmful now.** It races `imklaunchagent`: during a reinstall, the brief window where the LaunchAgent's old PID is being killed but its replacement hasn't published its Mach service yet, a host-app IMK lookup will trigger `imklaunchagent` to also spawn a fresh instance. Two processes end up both `bootstrap_register`'d on the Mach name. The user sees two identical IME entries in the macOS input-source menubar. Each host app is connected to whichever PID was alive at its first lookup, so killing "the duplicate" silently breaks every host app that was wired to the killed PID until those apps restart.
>
> **The clean architecture (canonical macOS IMK lifecycle):**
>
> 1. Install bundle to `~/Library/Input Methods/<AppName>.app` (atomic-swap during reinstall to prevent `HIToolbox`'s "bundle disappeared" enabled-state strip).
> 2. `lsregister -f` the install path (drops stray duplicates, asserts a single LS record).
> 3. After atomic swap: `lsregister -u` the staging-path inode BEFORE `rm -rf` (otherwise the old cdhash lingers in the LS dump as a phantom).
> 4. Sweep `lsregister -u` over any non-canonical `.app` under the project tree (iOS build products especially — Xcode's `xcodebuild -destination "iOS Device"` produces platform=iOS bundles that LaunchServices auto-registers and the macOS input-source picker enumerates).
> 5. Kill any running Inputx process. Next host-app use triggers `imklaunchagent` to lazy-spawn the new bundle.
> 6. Verify post-conditions: TIS row exists, `_ls_paths_for_bundle_id(BUNDLE_ID)` returns exactly one path (the install path). No PID check — there is no PID until the lazy spawn fires.
> 7. Validate binary health via a `probe` CLI subcommand on the binary itself (runs core logic, exits early, never instantiates IMKServer) — cheap, reliable, no race conditions.
>
> If you encounter the "silently refuses to launch" symptom that originally motivated this section, **debug it at the source**: enable private-data unification in the log (see [§3](#3-unredact-private-in-unified-log)) and check `process == "imklaunchagent"` for refusal messages. Common causes: missing Info.plist key, entitlement mismatch, code signature problem. Each is fixable directly; reaching for a LaunchAgent workaround layers a worse bug on top.
>
> The original LaunchAgent recipe follows for historical reference. **Do not use it on new installs.**

---

The final piece. After everything above is correct, `imklaunchagent` may *still* silently refuse to launch your binary on demand. The workaround: bypass it. Ship a user-level LaunchAgent that runs the binary continuously; the binary's own `IMKServer(name:)` publishes the Mach service and host apps connect directly.

Save as `~/Library/LaunchAgents/<bundle-id>.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.example.inputmethod.myime</string>
    <key>ProgramArguments</key>
    <array>
        <!-- ABSOLUTE path; expand $HOME via your install script. -->
        <string>/Users/USERNAME/Library/Input Methods/MyIME.app/Contents/MacOS/MyIME</string>
    </array>
    <key>RunAtLoad</key><true/>
    <key>KeepAlive</key>
    <dict><key>SuccessfulExit</key><false/></dict>
    <key>ProcessType</key><string>Interactive</string>
    <key>LimitLoadToSessionType</key><string>Aqua</string>
    <key>StandardOutPath</key><string>/tmp/myime.out.log</string>
    <key>StandardErrorPath</key><string>/tmp/myime.err.log</string>
</dict>
</plist>
```

Load it:

```bash
launchctl bootstrap "gui/$(id -u)" ~/Library/LaunchAgents/com.example.inputmethod.myime.plist
```

Verify:

```bash
pgrep -fl '/MyIME\.app/Contents/MacOS/MyIME$'
# Should print the binary path with a pid.

launchctl print "user/$(id -u)" | grep myime_Connection
# Should print: 0x<NNN>    U   A   com.example.inputmethod.myime_Connection
# U = User-published (the binary, not launchd), A = Active. This is the success state.
```

**Pitfalls — read these.**

1. **Do NOT add a `MachServices` key to the plist.** It tells launchd to claim the Mach service name; when your binary then `IMKServer(name:)` self-registers, the two collide. Flag flips to `M D` (launchd-managed) and the IME goes into a "switch-to-it-then-immediately-auto-skip-to-the-next" state. Let the binary register; launchd just runs the process.
2. **Do NOT include `~` in `ProgramArguments`.** launchd doesn't shell-expand. Use absolute `/Users/<name>/...` and substitute via your install script.
3. **`LimitLoadToSessionType=Aqua`** because the IME is a GUI-session service. Loading it in non-Aqua sessions wastes resources.

### `install.sh`

```bash
#!/usr/bin/env bash
set -euo pipefail
APP_NAME="MyIME"
BUNDLE_ID="com.example.inputmethod.myime"
APP_DST="$HOME/Library/Input Methods/$APP_NAME.app"
LA_DST="$HOME/Library/LaunchAgents/$BUNDLE_ID.plist"

# Tear down any running IME process + prior LaunchAgent registration.
launchctl bootout "gui/$(id -u)/$BUNDLE_ID" 2>/dev/null || true
pkill -x "$APP_NAME" 2>/dev/null || true

# Install bundle.
rm -rf "$APP_DST"
mkdir -p "$(dirname "$APP_DST")"
cp -R "./build/$APP_NAME.app" "$APP_DST"

# Register with TIS + enable each declared mode.
"$APP_DST/Contents/MacOS/$APP_NAME" install
killall TextInputMenuAgent 2>/dev/null || true

# Install + bootstrap LaunchAgent (templated copy from inside the .app).
mkdir -p "$(dirname "$LA_DST")"
sed "s|__APP_PATH__|$APP_DST|g" \
    "$APP_DST/Contents/Resources/LaunchAgent.plist.template" > "$LA_DST"
launchctl bootstrap "gui/$(id -u)" "$LA_DST"
```

### `.pkg` postinstall

```bash
#!/bin/bash
set -euo pipefail
APP="/Library/Input Methods/MyIME.app"
BUNDLE_ID="com.example.inputmethod.myime"
LSREG="/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister"

CONSOLE_USER=$(/usr/bin/stat -f%Su /dev/console)
CONSOLE_UID=$(/usr/bin/id -u "$CONSOLE_USER")
USER_HOME=$(/usr/bin/dscl . -read "/Users/$CONSOLE_USER" NFSHomeDirectory | awk '{print $2}')
USER_TMP=$(/bin/launchctl asuser "$CONSOLE_UID" /usr/bin/getconf DARWIN_USER_CACHE_DIR)

# Gate 1 — bundle registration + cache invalidation.
"$LSREG" -f "$APP" || true
/bin/rm -f "$USER_TMP/com.apple.IntlDataCache.le" "$USER_TMP/com.apple.IntlDataCache.le.kbdx"
/bin/launchctl asuser "$CONSOLE_UID" "$APP/Contents/MacOS/MyIME" install || true
/usr/bin/killall TextInputMenuAgent 2>/dev/null || true

# Gate 2 — LaunchAgent install for the console user.
LA_DST="$USER_HOME/Library/LaunchAgents/$BUNDLE_ID.plist"
LA_SRC="$APP/Contents/Resources/LaunchAgent.plist.template"
/bin/mkdir -p "$(dirname "$LA_DST")"
/usr/bin/sed "s|__APP_PATH__|$APP|g" "$LA_SRC" > "$LA_DST"
/usr/sbin/chown "$CONSOLE_USER:staff" "$LA_DST"
/bin/launchctl asuser "$CONSOLE_UID" /bin/launchctl bootout \
    "gui/$CONSOLE_UID/$BUNDLE_ID" 2>/dev/null || true
/bin/launchctl asuser "$CONSOLE_UID" /bin/launchctl bootstrap \
    "gui/$CONSOLE_UID" "$LA_DST"

exit 0
```

The cache delete removes the need for a logout for Gate 1. The LaunchAgent install removes the dependency on `imklaunchagent` for Gate 2.

That's the whole recipe. Apply it and the bundle appears in System Settings → Keyboard → Text Input → Add → (the language tab declared by `TISIntendedLanguage`), with the right display name and a real icon, on first install.

The rest of this article is how each rule was identified and which tools surfaced it. Skip to [Recap](#recap) if you don't care.

---

## Investigation — Gate 1

The interesting properties of the Gate 1 (picker enumeration) problem:

- Every diagnostic returns success. `codesign --verify`, `spctl --assess --type install`, `lsregister -dump`, `TISRegisterInputSource(bundleURL)`, `notarytool log` — all clean.
- The picker shows other third-party IMEs (Sogou, vChewing) that were installed before. The path works in general.
- No log line correlates with the rejection. `log show` is `<private>`-redacted across `com.apple.HIToolbox`, `com.apple.launchservices`, `com.apple.TextInputMenuAgent`.
- Apple's documentation doesn't apply: the IMK Programming Guide was last updated in 2007; the official sample (`NumberInput_IMKit_Sample`) shipped for Mac OS X 10.5; Xcode 15/16/17 ship no IME project template; no WWDC IMK session in the last seven years; the Apple Developer Forums `InputMethodKit` tag has two threads total, both unanswered.

The Gate 1 investigation went in five stages.

### 1. Compare against a working bundle

Install a known-working modern IME side by side and use it as ground truth. [vChewing 4.4.6](https://github.com/vChewing/vChewing-macOS/releases) was the reference; download the `.pkg`, `installer -pkg ... -target /`, picker shows it within seconds. This eliminates "macOS 26 is broken" as a hypothesis — the picker enumeration path works; the bundle is being rejected specifically.

Diff every observable surface:

```bash
# LaunchServices record
LSREG=/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister
$LSREG -dump | awk '/^bundle id:.*vChewing/,/^------/' > /tmp/ls_vchewing.txt
$LSREG -dump | awk '/^bundle id:.*MyIME/,/^------/'   > /tmp/ls_myime.txt
diff /tmp/ls_vchewing.txt /tmp/ls_myime.txt

# Info.plist keys
diff <(plutil -p ~/Library/Input\ Methods/vChewing.app/Contents/Info.plist) \
     <(plutil -p ~/Library/Input\ Methods/MyIME.app/Contents/Info.plist)

# Entitlements
codesign -d --entitlements - ~/Library/Input\ Methods/vChewing.app
codesign -d --entitlements - ~/Library/Input\ Methods/MyIME.app
```

Apply every visible difference as polish: `LSUIElement` (vChewing has it; my bundle had `LSBackgroundOnly`), `LSApplicationCategoryType`, `CFBundleIconFile` + a real `.icns` (`sips -s format icns input.tiff --out output.icns`), top-level `TISInputSourceID`, per-mode `TISInputSourceID` + `TISIntendedLanguage` + `smUnicode`, `app-sandbox` entitlement, per-locale `.lproj/InfoPlist.strings` with mode-id mapping. After applying every diff, the picker still ignored the bundle. Conclusion: the gate isn't in any of the observable bundle keys.

### 2. Extract HIToolbox from `dyld_shared_cache`

The TIS enumeration runs inside HIToolbox. On macOS 11+, system frameworks aren't standalone files on disk; the file at `/System/Library/Frameworks/Carbon.framework/Frameworks/HIToolbox.framework/Versions/A/HIToolbox` is a stub. Extract the real one:

```bash
brew install ipsw
DSC=/System/Volumes/Preboot/Cryptexes/OS/System/Library/dyld/dyld_shared_cache_arm64e
ipsw dyld extract "$DSC" HIToolbox --output /tmp/dsc_extract --objc --stubs
```

Grep for log format strings to learn the code paths:

```bash
strings -a /tmp/dsc_extract/HIToolbox | grep -E "TIS cache|AddInputMethod|TISFile|islc"
```

Output reveals the enumeration pipeline:

```
TIS cache rebuild log: TISFileInterrogator begin EUID=%u iNumberOfFileInfo %u ...
TIS cache rebuild log: AddInputMethodAppInfoForCache - CFBundleCopyInfoDictionaryInDirectory SUCCESS for %s
TIS cache rebuild log: AddInputMethodAppInfoForCache - CFBundleCopyInfoDictionaryInDirectory FAILED to get Info.plist for %s
TIS cache rebuild log: islcRebuildInputSourceCache (FSEvent dir-changed/SystemConfig logout notification) ...
```

The trace strings imply a `defaults` flag toggles them on:

```bash
defaults write -g AppleTISTraceCacheRebuild -bool YES
defaults write -g AppleTISTraceLogging -bool YES
killall cfprefsd
```

After enabling, `log stream --info --debug --predicate 'eventMessage CONTAINS "TIS cache rebuild"'` shows, on every TIS rebuild, exactly which bundles `TISFileInterrogator` processes. `iNumberOfFileInfo 9` for my session: 7 system IMEs + Sogou + vChewing. My bundle absent. The filter runs upstream of `AddInputMethodAppInfoForCache`, i.e., before any plist read. Whatever the gate is, no plist key change can fix it.

Disassemble for confirmation:

```bash
ipsw dyld disass "$DSC" --symbol-image HIToolbox --symbol _TISFileInterrogator --count 80
```

First call is to `__TISGetExtensionBundlesForIntlFileCacheUpdate` for NSExtension-based IMEs (the system `.appex` ones inside `/System/Library/Input Methods/*.app/Contents/PlugIns/`). Traditional `.app` IMEs come from a different file enumeration path.

### 3. Unredact `<private>` in unified log

Most HIToolbox / launchservices / TextInputMenuAgent log payloads come through as `<private>`. `log config --mode "private_data:on"` was removed in macOS 26. Apple's [Profiles and Logs](https://developer.apple.com/bug-reporting/profiles-and-logs/) library has no IME / Keyboard / HID / HIToolbox / Carbon profile.

Write a configuration profile with a `com.apple.system.logging` payload listing the subsystems to unredact. Save as `ime-debug.mobileconfig`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>PayloadType</key><string>Configuration</string>
    <key>PayloadVersion</key><integer>1</integer>
    <key>PayloadIdentifier</key><string>local.ime.debug.logging</string>
    <key>PayloadUUID</key><string>11111111-2222-3333-4444-555555555555</string>
    <key>PayloadDisplayName</key><string>IME debug logging</string>
    <key>PayloadScope</key><string>System</string>
    <key>PayloadContent</key>
    <array>
      <dict>
        <key>PayloadType</key><string>com.apple.system.logging</string>
        <key>PayloadIdentifier</key><string>local.ime.debug.logging.payload</string>
        <key>PayloadUUID</key><string>66666666-7777-8888-9999-aaaaaaaaaaaa</string>
        <key>PayloadVersion</key><integer>1</integer>
        <key>System</key><dict><key>Enable-Private-Data</key><true/></dict>
        <key>Subsystems</key>
        <dict>
          <key>com.apple.HIToolbox</key>
          <dict><key>DEFAULT-OPTIONS</key>
            <dict><key>Enable-Private-Data</key><true/>
                  <key>Persist-Debug</key><true/>
                  <key>Persist-Info</key><true/></dict></dict>
          <key>com.apple.launchservices</key>
          <dict><key>DEFAULT-OPTIONS</key>
            <dict><key>Enable-Private-Data</key><true/></dict></dict>
          <key>com.apple.TextInputMenuAgent</key>
          <dict><key>DEFAULT-OPTIONS</key>
            <dict><key>Enable-Private-Data</key><true/></dict></dict>
          <key>com.apple.inputmethodkit</key>
          <dict><key>DEFAULT-OPTIONS</key>
            <dict><key>Enable-Private-Data</key><true/></dict></dict>
          <key>com.apple.inputsources</key>
          <dict><key>DEFAULT-OPTIONS</key>
            <dict><key>Enable-Private-Data</key><true/></dict></dict>
        </dict>
      </dict>
    </array>
</dict>
</plist>
```

Double-click → System Settings → General → Device Management → Install. The "Unsigned Profile" warning is expected (self-authored). After install: `sudo killall logd cfprefsd`. `log stream` now returns real bundle paths instead of `<private>`. Remove the profile when done; it's a meaningful privacy reduction for the listed subsystems.

One trap in the now-readable logs: `linkd[xxx] [com.apple.appintents:Metadata] Rejecting com.example.inputmethod.myime at file:///..., is not trusted`. This fires for every third-party IME, including vChewing — `linkd` is the App Intents daemon and the rejection is about Apple Intents metadata, not picker enumeration. Verify by running the same install for vChewing and grepping the log; identical rejection. Red herring.

### 4. The sticky cache

After the polish in step 1, the disassembly in step 2, and the log unredaction in step 3: every test still came back with `0` entries for my bundle in the TIS database. Every test. Every variant. Identical result.

Too consistent to be the bundle. Something cached.

`TISCreateInputSourceList(nil, true)` from a Swift test script returned the same answer regardless of changes to `~/Library/Input Methods/`. The cache is per-process at minimum. To know where it lives on disk, trace the file accesses during a binary install:

```bash
# fs_usage requires sudo; route via osascript admin if no TTY:
sudo fs_usage -w -f filesys 2>&1 > /tmp/fs.out &
"$HOME/Library/Input Methods/MyIME.app/Contents/MacOS/MyIME" install
sleep 3; kill %1
grep -iE 'intl|component|hitoolbox' /tmp/fs.out
```

Two files surface:

```
/private/var/folders/<random>/<random>/C/com.apple.IntlDataCache.le        (8.8 KB)
/private/var/folders/<random>/<random>/C/com.apple.IntlDataCache.le.kbdx   (167 KB)
```

That's the user's temp cache dir (`getconf DARWIN_USER_CACHE_DIR`). The sizes are right for ~70 input source records. Delete them:

```bash
rm -f "$(getconf DARWIN_USER_CACHE_DIR)/com.apple.IntlDataCache.le"*
```

Next `TISCreateInputSourceList` triggers:

```
TISFileInterrogator updateSystemInputSources false but old data invalid:
    currentCacheHeaderPtr nonNULL? 0, ->cacheFormatVersion 0, ->magicCookie 00000000, ...
```

Cache miss, full rebuild, file_info enumeration fires, results land in the new cache file. With cache invalidation in the iteration protocol, every change to the bundle becomes actually testable.

Why the cache is invisible to other invalidation paths: `killall TextInputMenuAgent` respawns the agent which then reads the on-disk cache. `lsregister -f` updates LaunchServices but doesn't touch IntlData. `TISUpdateIntlFileCache()` (private symbol exported from HIToolbox, callable via `dlsym`) does check an internal "needs update" flag but takes no action if the flag is unset, which it usually is. Distributed notifications get delivered but the listener checks the same flag. The only reliable invalidation is deleting the file.

### 5. Bisect to find the gate

Iteration protocol now reliable. Take the polished bundle (passes), revert one atom at a time, clean install + cache delete + scan, log pass/fail.

```bash
#!/bin/bash
ATOM="$1"
APP=~/Library/Input\ Methods/MyIME.app
osascript -e 'do shell script "rm -rf '"$APP"'" with administrator privileges' >/dev/null 2>&1
cp -R ./build/MyIME.app "$APP"
rm -f "$(getconf DARWIN_USER_CACHE_DIR)/com.apple.IntlDataCache.le"*
"$APP/Contents/MacOS/MyIME" install >/dev/null 2>&1
sleep 1
RESULT=$(swift scan_tis.swift | grep 'inputx hits:' | awk '{print $NF}')
echo "$ATOM: hits=$RESULT"
```

Revert each atom in turn (with the others kept polished), rebuild, run the script. Nine tested:

| Atom reverted | hits |
|---|---|
| Top-level `TISInputSourceID` removed | 2 |
| `LSUIElement` → `LSBackgroundOnly` | 2 |
| `app-sandbox` entitlement removed | 2 |
| Per-mode `TISInputSourceID` / `TISIntendedLanguage` / `smUnicode` removed | 2 |
| `CFBundleIconFile` + `.icns` removed | 2 |
| Bundle owned by user (no `chown root`) | 2 |
| `binary install` self-register skipped | 2 |
| `.lproj` mode-id mapping removed | 2 |
| `CFBundleIdentifier` from `com.example.inputmethod.myime` → `com.example.myime` | **0** |

The bundle id `inputmethod` infix is the gate. Every other "polish" atom is unrelated to enumeration; each one fixes a different UX detail.

Two earlier attempts at the bundle-id experiment (before discovering the cache) had returned the same 0-hits result for a bundle id *that did contain* `inputmethod` — because the cache was stale. The signal was always there, the testing rig had been hiding it.

---

## Investigation — Gate 2 (IMK service launch)

Gate 1 makes the IME *appear* in the picker. Gate 2 is what makes it *type*. They're independent: every Gate 1 fix can be in place, the picker enumerates the IME, the user can switch to it, and nothing happens when they type. The Gate 2 investigation kicked off after Gate 1 was shipped and "the picker shows it!" turned out not to actually mean "the IME works".

### 6. Discover the "switches but doesn't type" failure mode

After Gate 1 was solved, the next test was simply: open WeChat, switch to my IME, type. WeChat **crashed**:

```
Process: WeChat
Exception Type: EXC_BREAKPOINT (SIGTRAP)
Application Specific Information:
*** CFRelease() called with NULL ***

Thread 0 Crashed:
0  CoreFoundation         ___CFRelease.cold.2 + 16
1  CoreFoundation         ___CFRelease + 344
2  TextInputUIMacHelper   -[TUINSCursorUIController _selectCurrentInputSource] + 84
3  TextInputUIMacHelper   -[TUINSCursorUIController moveTextInputMenuHUD:] + 272
4  HIToolbox              TSMMessagePortCallBack + 144
```

The host app's TextInputUIMacHelper is dereferencing NULL very early in `_selectCurrentInputSource` (offset `+ 84`) — most likely a `TISCopy*` call returning nil for a property the helper unconditionally releases. Verified by switching to vChewing in the same WeChat session: no crash, IME works. So it's something specific my bundle returns NULL for.

Diff vs vChewing's resources:

```
$ ls vChewing.app/Contents/Resources/ | grep -i icon
AppIcon.icns
Assets.car

$ ls MyIME.app/Contents/Resources/ | grep -i icon
myime_app_icon.icns
myime_menu_icon.tiff

$ file vChewing.app/Contents/Resources/AppIcon.icns
Mac OS X icon, 51473 bytes, "ic13" type

$ file MyIME.app/Contents/Resources/myime_app_icon.icns
Mac OS X icon, 1120 bytes, "TOC " type
```

There's the problem. vChewing ships a 51 KB modern `ic13` `.icns`; mine is a 1.1 KB legacy `TOC` / `il32` blob produced by `sips -s format icns input.tiff`. `IconRef` returns NULL for the legacy single-resolution form on macOS 26; host's TextInputUIMacHelper doesn't NULL-check before releasing.

Rebuild as a proper multi-resolution iconset (see [Skeleton — `.icns`](#multi-resolution-icns)) → 137 KB modern `ic12`. Reinstall. Switch in WeChat again. **No crash.** But typing still does nothing — now it's the cleaner symptom: "switches OK, no crash, but no characters appear when I type."

### 7. Locate the imklaunchagent refusal

`pgrep -fl Inputx` while the IME is supposedly active in a host app: nothing. The binary was never launched. `IMKServer` never ran, no Mach service was published, the host app's IMK client has nothing to connect to. So whose decision is "don't launch this binary"?

Broad-net log query:

```bash
log show --last 60s --predicate 'process == "imklaunchagent" \
    OR composedMessage CONTAINS "InputMethodConnectionName"'
```

Hits:

```
14:26:14.285 imklaunchagent[434] [com.apple.inputmethodkit:Server]
    Refusing connection name for bundle: unrecognized 'InputMethodConnectionName' value
```

`imklaunchagent` is the decision-maker. Find it on disk (it's NOT in `dyld_shared_cache`; it's a standalone Mach-O at `/System/Library/Frameworks/InputMethodKit.framework/Resources/imklaunchagent`) and `strings` both it and the IMK framework:

```bash
strings -a /System/Library/Frameworks/InputMethodKit.framework/Resources/imklaunchagent \
    | grep -iE 'connection|inputmethod|valid'
# isValidBundleIdentifier:
# connectionNameFor:
# .inputmethod.
# kInputMethodExecutablePathKey
# kInputMethodBundleIdentifierKey
# kInputMethodNeedSandboxExtensionKey
# kInputMethodIsNSExtensionKey

strings -a /tmp/dsc_extract/InputMethodKit \
    | grep -iE 'refusing|allowedinput|connection'
# Refusing connection name for bundle: invalid bundleIdentifier
# Refusing connection name for bundle: unrecognized 'InputMethodConnectionName' value
# _allowedInputMethodConnectionNames
# SCIM_1_Connection
# TCIM_1_Connection
# com.apple.inputmethod.JIM.IMK_Connection
# com.apple.inputmethod.Ainu.IMK_Connection
```

The framework has an `_allowedInputMethodConnectionNames` method that gates which connection names IMK will launch. The hardcoded strings visible in the binary are Apple-internal IMEs only; third-party bundles must reach the launch path via some other rule the strings don't expose.

### 8. Match vChewing's bundle config bit-for-bit

vChewing is the modern third-party IME that demonstrably works on this Mac. Diff its `Info.plist` against mine:

```
$ diff \
    <(plutil -p vChewing.app/Contents/Info.plist | grep -oE '"[A-Z][a-zA-Z]+"' | sort -u) \
    <(plutil -p MyIME.app/Contents/Info.plist    | grep -oE '"[A-Z][a-zA-Z]+"' | sort -u)
< "InputMethodServerDataSourceClass"
< "InputMethodSessionController"
```

vChewing has two `InputMethod*` keys I don't. Both undocumented (zero hits on developer.apple.com for either name in 2026); both apparently required by `imklaunchagent`. Add them to `Info.plist`, point all four `InputMethodServer*` + `InputMethodSessionController` at the same Swift class.

Diff entitlements:

```
$ codesign -d --entitlements - vChewing.app 2>&1 | grep '<key>'
<key>com.apple.security.app-sandbox</key>
<key>com.apple.security.files.bookmarks.app-scope</key>
<key>com.apple.security.files.user-selected.read-write</key>
<key>com.apple.security.network.client</key>
<key>com.apple.security.temporary-exception.files.home-relative-path.read-only</key>
<key>com.apple.security.temporary-exception.files.home-relative-path.read-write</key>
<key>com.apple.security.temporary-exception.mach-register.global-name</key>
<key>com.apple.security.temporary-exception.shared-preference.read-only</key>
```

Eight entitlements; mine had two. The two `home-relative-path` exceptions are vChewing-specific paths (its Yahoo KeyKey dictionary import + iCloud sync). The other six are generic IME requirements — add them.

Also rule-out: notarize + staple the bundle. `spctl --assess --verbose=4 --type install` should report `source=Notarized Developer ID`, not `Unnotarized Developer ID`. macOS 26's IMK launch path rejects unnotarized signing.

After all of the above: `imklaunchagent` no longer logs `Refusing`. But the binary still doesn't get launched on switch. Same end result, no crash, no typing.

### 9. LaunchAgent workaround

Side-by-side test on the same Mac with both IMEs installed: switch to vChewing in a text field, type — works. Switch to my IME, type — nothing. `launchctl list | grep -iE 'inputmethod\.'` shows vChewing's IMK service running but mine missing. Whatever `imklaunchagent`'s opaque allow/refuse logic checks, my bundle still fails it even with config that visibly matches vChewing's.

Bypass it. Launch my binary manually:

```bash
"$HOME/Library/Input Methods/MyIME.app/Contents/MacOS/MyIME" &
launchctl print "user/$(id -u)" | grep myime_Connection
# 0x119bfb    U   A   com.example.inputmethod.myime_Connection
```

`U A` = User-published, Active. The binary self-registered the Mach service. Switch to the IME in a text field, type — **it works.** So the binary is fully functional; the only piece missing is the launch trigger.

Solution: ship a user-level LaunchAgent (`~/Library/LaunchAgents/<bundle-id>.plist`) with `RunAtLoad=true` and `KeepAlive`. The binary stays running across logins; the IMK Mach service is permanently published; host apps connect to it directly; `imklaunchagent`'s decision becomes irrelevant.

Two pitfalls each cost an hour:

- **Don't add `MachServices` to the LaunchAgent plist.** It tells launchd to claim the Mach service name; the binary's `IMKServer(name:)` then collides. Flag flips from `U A` to `M D`, and the IME goes into a "switch-to-it-then-immediately-auto-skip" failure mode. Let the binary self-register; the LaunchAgent just runs the process.
- **Don't `killall cfprefsd` for any reason while debugging.** `cfprefsd` holds `AppleEnabledInputSources` in RAM; killing it wipes the user's enabled-IME list. The IME is then still "installed" per System Settings, but doesn't appear in the menu-bar input switcher. The user has to re-Add it via System Settings UI (re-triggering the third-party IME permission popup) to repopulate the list. Cost four iterations in the original investigation before I realized what was wiping the state.

The LaunchAgent's `ProgramArguments` needs an absolute path (`~` is not shell-expanded by launchd). Substitute the value via your install script ([Skeleton — `install.sh`](#installsh)) or `.pkg` postinstall ([Skeleton — `.pkg` postinstall](#pkg-postinstall)).

After the LaunchAgent loads and the user Adds the IME via System Settings: typing works end-to-end. That's the moment two days of investigation finally yield one character.

### What's still unknown

I did not isolate the actual root cause of `imklaunchagent`'s residual refusal. Both vChewing's bundle and mine satisfy every visible check — bundle id pattern, all 4 IMK keys, all 6 entitlements, Notarized DevID signing, hardened runtime, `@objc(...)` controller. Something else gates the launch decision and I couldn't see it from the disassembly. The LaunchAgent is a clean bypass, but it's a workaround, not the proper fix. If you isolate the actual check `imklaunchagent` runs, please file an issue on this article and I'll update.

---

## Investigation — Gate 3 (picker tile rendering)

Gate 1 enumerates your IME. Gate 2 makes it type. Gate 3 is what makes **System Settings → Keyboard → Input Sources → Add** (and the Ctrl+Space switcher HUD) paint a proper tile next to your IME's name — Apple ABC shows a dark chip with a white "A", Sogou WB shows its white-chip + black "S" logo. A naive third-party bundle gets a blank chip outline, or worse, a one-color solid tile where chip and ink fuse into the same fill.

The interesting properties of the Gate 3 problem:

- The failure is **visible**, not silent. The tile is *drawn* — just wrong. Empty outline, outline-only "五" with white fill, solid blue chip on selection. Each broken state is a clue about which rendering path the picker picked.
- TIS API metadata is perfect throughout. `TISCreateInputSourceList` returns your IME, `kTISPropertyIconImageURL` points at your tiff, file exists, size matches. Picker has the data; it just draws it wrong.
- Rendering doesn't dispatch on `TISIconIsTemplate`. It dispatches on the **shape of `ComponentInputModeDict.tsInputModeListKey.<mode-id>` — which keys are present** — plus the pixel composition of the tiff. The two must be consistent.
- Apple's own SCIM (Pinyin/Wubi/Stroke) and Sogou WB use **two completely different recipes that both work**. Mixing them — which we did initially, taking the `TIS*` keys from SCIM and the chip-design tiff from Sogou — puts picker into a broken intermediate state.

### Two valid recipes

| Aspect | Apple-template (SCIM) | Third-party-chip (Sogou) |
|---|---|---|
| Mode dict has | `TISInputSourceID`, `TISIntendedLanguage`, `TISIconLabels.Primary`, `tsInputModeCharacterRepertoireKey` | none of those, but has `tsInputModeAlternateMenuIconFileKey`, `tsInputModeDefaultStateKey`, `tsInputModeKeyEquivalentKey` (can be ""), `tsInputModeKeyEquivalentModifiersKey` (= 4608) |
| Top-level `TISIconIsTemplate` | `true` | (omit entirely) |
| TIFF | Alpha-on-transparent. All RGB=(0,0,0), only alpha varies (0 = transparent, 255 = ink). No chip background. Apple's `wubixing.tiff` has 4 corner pixels at alpha=6, center at alpha=238. | White solid rounded chip + black ink. Chip edges are RGB=255 with alpha gradient (PIL LANCZOS produces (255,255,255,142) naturally). Sogou's chip edges are RGB=0 alpha=128 — same idea, different convention. |
| Picker draws | A dark chip frame itself, tints the alpha mask to white text on top. Selection state recolors the chip. | The tiff bytes as-is. Your chip is what shows. Selection state composites a transparent blue overlay on top. |

The picker dispatches on the mode dict keys: presence of `TISIconLabels` (or any `TIS*` mode-dict key) → Apple-template path. Absence of all `TIS*` mode-dict keys + presence of the `tsInputModeKeyEquivalentKey/Modifiers` pair → third-party-chip path. Default fallback (when picker can't classify) → render an empty chip outline.

### The investigation path

After Gates 1 + 2 were solved and Inputx was actually usable, the picker tile next to "Inputx 五笔" in **System Settings → Keyboard → Input Sources → Add** was a blank chip outline. ABC and Sogou both rendered correctly in the same dialog. The TIFF was a 16×16 / 32×32 multipage from `tiffutil -cathidpicheck`, matching Apple TamilIM's tiff structure bit-for-bit. The plist had `tsInputModePaletteIconFileKey` pointing at the right file. Nothing else was obviously wrong.

Eleven iterations to figure out which lever does what:

1. **Set `TISIconLabels.Primary = "五笔"`.** No change.
2. **Set `TISIconIsTemplate = true`.** No change.
3. **Fix `tsInputModeScriptKey` from `smUnicode` (not a real ScriptCode) to `smSimpChinese`** (matches SCIM Pinyin). No change.
4. **Delete `TISIconLabels`** (testing whether picker walks tiff path when label is absent). No change.
5. **Switch tiff from alpha-on-transparent black ink to white-chip + black-ink + opaque gray border (RGB=160,160,160 alpha=255).** → Tile **renders for the first time**, but as **outline-only "五"** — picker treats opaque gray edge pixels as a separate stroke channel and only paints outlines.
6. **Remove gray outline; build chip via supersample-LANCZOS** so edges are RGB=255 with alpha gradient (no opaque non-white-non-black pixels). → Tile renders as **solid blue chip** when the row is selected. Picker now thinks the whole tiff is an alpha mask and tints chip-bg + ink uniformly to selection color.
7. **Pixel-level diff our tiff vs Sogou's** with `Counter(img.getdata())`. Sogou's edge antialias is RGB=(0,0,0) alpha=128 (half-transparent black). Ours is RGB=(255,255,255) alpha=142 (half-transparent white). Both gradient-not-opaque, but picker still treats them differently.
8. **Diff every key in our `ComponentInputModeDict` against Sogou's.** Sogou's mode dict has 9 keys, 4 of which are unique to it (`AlternateMenuIconFileKey`, `DefaultStateKey`, `KeyEquivalentKey`, `KeyEquivalentModifiersKey`). Ours has 9 too but mixed in `TISInputSourceID` + `TISIntendedLanguage` + `tsInputModeCharacterRepertoireKey` from SCIM.
9. **Mirror Sogou's mode dict bit-for-bit.** Remove `TISInputSourceID` + `TISIntendedLanguage` + `tsInputModeCharacterRepertoireKey` from mode dict. Add `tsInputModeAlternateMenuIconFileKey` + `tsInputModeDefaultStateKey` + `tsInputModeKeyEquivalentKey="" `+ `tsInputModeKeyEquivalentModifiersKey=4608`. Also delete top-level `TISIconIsTemplate` (Sogou doesn't set it). → **Tile renders correctly.** White chip + black "五", matching Sogou's "S" tile in the same dialog.
10. **Glyph polish — heavier weight for legibility.** First pass used STHeiti Medium; "五" rendered as a thin gray smear at 16×16 because antialiasing leaves too little black ink. Switched to Hiragino Sans W9 (heaviest CJK macOS ships). Glyph went solid black but felt heavy. Switched again to PingFang SC Medium (`PingFang.ttc` index 7) for a more PingFang/macOS-native look.
11. **Drop the alpha hardening step**. While Hiragino was the glyph font I added `alpha = a.point(lambda v: 255 if v >= 160 else (v*1.4 if v >= 60 else 0))` to force gray antialias pixels to solid black. With PingFang Medium this stairsteps the edge instead of smoothing it. Removing the step lets LANCZOS produce clean antialiased edges that read as solid black at display size without looking jagged on close inspection.

Final asset: [`mac/Resources/inputx_menu_icon.tiff`](https://github.com/goliajp/inputx/blob/develop/mac/Resources/inputx_menu_icon.tiff), 16×16 @72dpi + 32×32 @144dpi LZW multipage, white chip + LANCZOS-antialiased PingFang SC Medium "五" at inkbox 12/24 (~75% of canvas). Reproducible from [`mac/generate_menu_icon.py`](https://github.com/goliajp/inputx/blob/develop/mac/generate_menu_icon.py).

### Two new mental models from this gate

1. **Picker has at least two rendering paths**, dispatched by mode-dict key presence, not by `TISIconIsTemplate`. Mix the paths → undefined intermediate state → broken tile. Pick one recipe (chip-style if you want full control over visual, template-style if you want system tinting / dark-mode adaptation) and **don't mix `TIS*` mode-dict keys with `ts*KeyEquivalent*` keys** in the same mode.
2. **The TIFF pixel composition has to match the chosen recipe.** Apple-template path expects RGB=0 everywhere, alpha varies. Third-party-chip path expects RGB=255 in chip body, RGB=0 in ink, antialias edges with alpha gradient on RGB=255 (or RGB=0). **Never opaque RGB=128 mid-gray pixels** — picker classifies those as stroke-only commands and renders outlines.

### What's still unknown

I didn't extract the picker UI code to confirm the dispatch logic — the empirical "presence of `TIS*` mode keys = template path, absence + `KeyEquivalent` pair = chip path" rule is reverse-engineered from how rendering changes when keys are toggled. The actual classifier may be more or less precise. If you isolate the source code branch, please file an issue.

Also: the Ctrl+Space switcher HUD (the floating picker that appears when holding Control and tapping Space) reads from the same plist + tiff and should render the same chip — but on my macOS 26.5 it didn't update until `TextInputSwitcher` was killed via `kill -9 $PID` directly (not `killall`, which silently fails on SIP-protected system services). After force-kill, launchd respawns it and the new instance reads fresh assets. Add this to your asset-refresh dance: `kill -9 $(pgrep -x TextInputSwitcher)` alongside `kill -9 $(pgrep -x TextInputMenuAgent)`.

### Gate 3.5 — `CFBundleIconName` makes Ctrl+Space switcher read your app .icns instead of menu .tiff

Even with mode dict mirrored to Sogou and the menu .tiff rendering correctly in the Add Input Source dialog, the **Ctrl+Space switcher HUD** kept showing my `.icns` icon shrunk to 16×16 — the navy-bg + white "笔" + W/P dot from `inputx_app_icon.icns`, scaled down to where the "笔" became an unrecognizable smudge that read as "λ" or "入" depending on the angle.

The difference between picker contexts:

- **Add Input Source dialog tile**: reads `tsInputModePaletteIconFileKey` (.tiff). Works once mode dict is right.
- **Ctrl+Space switcher HUD**: on macOS 26, reads `CFBundleIconName` *first* if it's set, resolves to the named asset catalog or .icns. Only falls through to `tsInputModePaletteIconFileKey` if `CFBundleIconName` is absent.

Sogou's plist has `CFBundleIconFile = "sogou.icns"` but **no `CFBundleIconName`** — so the Ctrl+Space switcher walks the fallback chain and ends up at `tsInputModeMenuIconFileKey` (`sogou_menu_icon.tiff`). Ours had both `CFBundleIconFile = "inputx_app_icon"` and `CFBundleIconName = "inputx_app_icon"`, so the switcher resolved the modern-style asset reference and never reached the .tiff.

**Fix**: delete `CFBundleIconName` from `Info.plist`. Keep `CFBundleIconFile` (Dock/Finder use it). The Ctrl+Space switcher now reads the menu .tiff and shows the same chip as the Add Input Source dialog.

This trap doesn't show up in Apple's IMK documentation because the 2007 guide doesn't mention `CFBundleIconName` at all (it's a macOS 11+ asset catalog convention added for Dock/Finder integration). Apple's own SCIM/TamilIM bundles also don't set it. The trap is invisible until you encounter both surfaces and notice they're rendering different things.

---

## Debug knobs to know

- **`AppleTISTraceCacheRebuild`** — HIToolbox's own verbose trace. No profile required.
  ```bash
  defaults write -g AppleTISTraceCacheRebuild -bool YES
  defaults write -g AppleTISTraceLogging -bool YES
  killall cfprefsd
  log stream --info --debug --predicate 'eventMessage CONTAINS "TIS cache rebuild"'
  ```
- **`.mobileconfig` with `com.apple.system.logging` payload** — unredact `<private>` for the chosen subsystems. Template in §3 above.
- **`ipsw dyld extract` + `strings -a` + `ipsw dyld disass`** — reverse the relevant framework. The 2007 docs only cover a fraction of what HIToolbox actually does.
- **`fs_usage -w -f filesys`** — find caches and other on-disk state touched during runtime.
- **`getconf DARWIN_USER_CACHE_DIR`** — the user temp dir where `com.apple.IntlDataCache.le[+.kbdx]` lives.

---

## Recap

### Gate 1 — picker enumeration

- **`CFBundleIdentifier` contains `inputmethod`** (substring). Convention: `<reverse-dns>.inputmethod.<short-name>`.
- **Delete `$(getconf DARWIN_USER_CACHE_DIR)/com.apple.IntlDataCache.le[+.kbdx]`** to force TIS to re-enumerate after any bundle change. Include this step in `install.sh` (for dev) and `.pkg` postinstall (for users).

### Gate 2 — IMK service launch

- **Four `Info.plist` IMK keys**: `InputMethodServerControllerClass`, `InputMethodServerDelegateClass`, **`InputMethodServerDataSourceClass`**, **`InputMethodSessionController`**. The last two are macOS 26-undocumented; missing either makes `imklaunchagent` silently refuse to launch your binary.
- **Real multi-resolution `.icns`** via `iconutil -c icns iconset/` from a proper 16/32/64/128/256/512 + @2x iconset. NOT `sips -s format icns input.tiff` (1-resolution legacy blob crashes hosts on switch via `CFRelease(NULL)`).
- **Six entitlements** (not the bare two): `app-sandbox` + `files.bookmarks.app-scope` + `files.user-selected.read-write` + `network.client` + `temporary-exception.mach-register.global-name` + `temporary-exception.shared-preference.read-only`.
- **Notarized + stapled Developer ID** (unnotarized is refused by the IMK launch path).
- **`@objc(<YourController>)`** annotation on the Swift `IMKInputController` subclass.
- **User-level `KeepAlive` LaunchAgent** at `~/Library/LaunchAgents/<bundle-id>.plist` that runs the IME binary continuously. Bypasses `imklaunchagent`'s opaque refusal. *Do NOT declare `MachServices` in the plist* (causes "switch-then-auto-skip" via Mach name conflict with `IMKServer(name:)`).

### Gate 3 — picker tile rendering

- **Pick one rendering path and don't mix.** Apple-template (alpha-on-transparent + `TISIconLabels` + `TISInputSourceID` in mode dict + `TISIconIsTemplate=true`) vs third-party-chip (white-chip tiff + `tsInputModeAlternateMenuIconFileKey` + `tsInputModeDefaultStateKey` + `tsInputModeKeyEquivalentKey="" `+ `tsInputModeKeyEquivalentModifiersKey=4608`, with NONE of the `TIS*` mode keys and NO top-level `TISIconIsTemplate`).
- **TIFF for chip path**: 16×16 @72 + 32×32 @144 multipage LZW, white solid rounded rect + black ink + LANCZOS-antialiased edges. Build chip at 8× supersample then downscale (PIL: `Image.new(RGBA, (size*8, size*8))` + `ImageDraw.rounded_rectangle(fill=(255,255,255,255))` + `Image.LANCZOS`). **Never opaque mid-gray pixels** (RGB=128 alpha=255) — picker classifies these as stroke commands and renders outlines only.
- **No alpha hardening on the glyph.** LANCZOS downscale of supersampled ink produces clean antialiased edges; a `if alpha > threshold` step function stairsteps the edge and looks worse.
- **`kill -9 $(pgrep -x TextInputSwitcher)`** after a TIFF swap to refresh the Ctrl+Space switcher HUD cache. `killall TextInputSwitcher` silently fails on SIP-protected system services on macOS 26; `kill -9` by PID works (launchd respawns it).
- **No `CFBundleIconName`** in `Info.plist` — otherwise Ctrl+Space switcher HUD resolves icon via modern asset-catalog path → `.icns` instead of falling through to `tsInputModePaletteIconFileKey` → `.tiff`. Keep `CFBundleIconFile` for Dock/Finder; remove `CFBundleIconName` to free the menu/picker fallback chain.

### Process notes

- **Don't `killall cfprefsd`** during IME debugging — wipes user's `AppleEnabledInputSources`, forcing re-Add through System Settings UI.
- After Gate 2 is in place, verify with `pgrep -fl <YourBinary>` (should show alive) + `launchctl print user/$(id -u) | grep <conn-name>` (should show `U A` flag).

### Diagnostics worth installing once and forgetting about until the next IME bug

- `defaults write -g AppleTISTraceCacheRebuild -bool YES` + `defaults write -g AppleTISTraceLogging -bool YES`
- `com.apple.system.logging` profile for `<private>` unredaction
- `ipsw` for extracting frameworks from `dyld_shared_cache`
- `fs_usage -w -f filesys` for finding sticky caches under runtime activity

---

## References

- **[Inputx](https://github.com/goliajp/inputx)** (MIT) — reference codebase. Live working files:
  - [`mac/Info.plist`](https://github.com/goliajp/inputx/blob/develop/mac/Info.plist) — all 4 IMK keys
  - [`mac/Inputx.entitlements`](https://github.com/goliajp/inputx/blob/develop/mac/Inputx.entitlements) — 6-entitlement set
  - [`mac/Resources/LaunchAgent.plist.template`](https://github.com/goliajp/inputx/blob/develop/mac/Resources/LaunchAgent.plist.template) — Gate-2 LaunchAgent template
  - [`mac/Resources/inputx_app_icon.icns`](https://github.com/goliajp/inputx/blob/develop/mac/Resources/inputx_app_icon.icns) — real multi-res .icns example
  - [`mac/Resources/inputx_menu_icon.tiff`](https://github.com/goliajp/inputx/blob/develop/mac/Resources/inputx_menu_icon.tiff) — Gate-3 chip-style menu/picker icon (16×16 + 32×32 multipage)
  - [`mac/generate_menu_icon.py`](https://github.com/goliajp/inputx/blob/develop/mac/generate_menu_icon.py) — reproducible script for the chip-style tiff
  - [`mac/Sources/main.swift`](https://github.com/goliajp/inputx/blob/develop/mac/Sources/main.swift)
  - [`mac/install.sh`](https://github.com/goliajp/inputx/blob/develop/mac/install.sh) — Gate-1 install + Gate-2 LaunchAgent bootstrap
  - [`mac/pkg/scripts/postinstall`](https://github.com/goliajp/inputx/blob/develop/mac/pkg/scripts/postinstall) — same as above, .pkg-flavored
- **[vChewing](https://github.com/vChewing/vChewing-macOS)** — Taiwanese Bopomofo IME. Used as the reference working bundle for diff in §1.
- **[gureum](https://github.com/gureum/gureum)** — Korean IME, modern Swift.
- **[fcitx5-macos](https://github.com/fcitx-contrib/fcitx5-macos)** — general IME framework, includes Chinese/Japanese/Korean/Vietnamese.
- **[InputMethodKit Release Notes (2007)](https://developer.apple.com/library/archive/releasenotes/Cocoa/RN-InputMethodKit/index.html)** — the only Apple doc, still useful for IMK fundamentals.
- **[ipsw](https://blacktop.github.io/ipsw/)** — `dyld_shared_cache` extraction + Mach-O disassembly.

This article is on GitHub at [goliajp/inputx/docs/macos-ime-recipe-2026.md](https://github.com/goliajp/inputx/blob/develop/docs/macos-ime-recipe-2026.md). Issues and PRs welcome.
