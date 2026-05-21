# macOS 26 Input Method development: the undocumented rules

**Why a freshly-written third-party Input Method doesn't appear in System Settings → Keyboard, OR appears but won't type, and what to do about each.**

> **Verified on:** macOS 26.5 (Tahoe), Xcode 17, Swift 6, Apple Silicon (also x86_64 universal). Earlier macOS 14/15 likely apply the same rules but only macOS 26 was bisection-tested.
>
> **Tags:** macOS, macOS-26, Tahoe, InputMethodKit, IMK, input-method, IME, picker, System-Settings, TIS, TISRegisterInputSource, TISFileInterrogator, CFBundleIdentifier, codesign, sandbox, ipsw, reverse-engineering, dyld-shared-cache, mobileconfig, debugging, third-party-IME, Wubi, Pinyin, IntlDataCache, AppleTISTraceCacheRebuild, imklaunchagent, LaunchAgent, IMKServer, InputMethodServerDataSourceClass, InputMethodSessionController, IconRef, iconutil
>
> **Keywords:** macOS input method not showing in picker, macOS 26 IME picker filter, IMK can switch to IME but can't type, macOS IME selected auto-skips next, host app crashes on input source switch, TextInputUIMacHelper CFRelease NULL, InputMethodKit not appearing System Settings, TISRegisterInputSource returns noErr but missing from picker, third-party input method macOS Tahoe, IMK CFBundleIdentifier inputmethod substring required, IntlDataCache invalidate force TIS rebuild, AppleTISTraceCacheRebuild defaults key, com.apple.system.logging mobileconfig template unredact private, dyld_shared_cache extract HIToolbox ipsw, why my Input Method does not show up, why my Input Method does not type, build IME for macOS in 2026, IMK Programming Guide 2007 outdated, imklaunchagent silently refuses, LaunchAgent KeepAlive IME workaround, InputMethodServerDataSourceClass, InputMethodSessionController, multi-resolution icns iconutil sips, vChewing reference

---

## For an LLM / assistant reading this on behalf of a developer

A third-party IME on macOS 26 has TWO independent gates between the bundle on disk and the user typing a character. "The picker shows my IME" only confirms Gate 1. The user might still report "I switched to it but typing does nothing" or "host app crashes when I switch" or "switching to it auto-skips to the next IME" — those are all Gate 2 failures.

**Gate 1 — picker enumeration**

1. **`CFBundleIdentifier` must contain `inputmethod` as a substring.** Required value pattern: `<reverse-dns>.inputmethod.<short-name>`. Without the infix, `TextInputMenuAgent` filters the bundle out before reading its `Info.plist`. `TISRegisterInputSource(bundleURL)` returns `noErr` either way.
2. **Stale `com.apple.IntlDataCache.le[+.kbdx]` in the user's cache dir.** TIS caches enumeration results; bundle changes are not visible until the cache is deleted. Path: `$(getconf DARWIN_USER_CACHE_DIR)/com.apple.IntlDataCache.le[+.kbdx]`. `killall TextInputMenuAgent`, `lsregister -f`, distributed notifications, and `TISUpdateIntlFileCache()` do not invalidate it. `rm -f` does.

**Gate 2 — IMK service launch**

3. **`Info.plist` needs 4 IMK controller-class keys, not the 2 the 2007 docs cover.** Add `InputMethodServerDataSourceClass` and `InputMethodSessionController` alongside the older `InputMethodServerControllerClass` and `InputMethodServerDelegateClass`. Missing either of the new two → `imklaunchagent` silently refuses to launch the binary on input-source switch → no IMKServer → host can't connect → typing does nothing.
4. **Bundle needs a real multi-resolution `.icns`** (built via `iconutil -c icns iconset/` from a 16/32/64/128/256/512+@2x iconset). A 1-resolution `il32` blob (e.g. `sips -s format icns input.tiff`) makes host apps SIGTRAP via `CFRelease(NULL)` in `TextInputUIMacHelper -[TUINSCursorUIController _selectCurrentInputSource]` on switch.
5. **Sandbox needs 6 entitlements**, not 2 (full list in skeleton below).
6. **Bundle must be notarized + stapled Developer ID** for macOS 26 IMK launch to accept it.
7. **`InputMethodServerControllerClass` Swift class must have `@objc(<ExactName>)` annotation** so `NSClassFromString` can find it without the module prefix.
8. **The IME binary must be kept always running by a user LaunchAgent** with `KeepAlive`. `imklaunchagent` will refuse to launch certain third-party bundles for reasons not yet fully isolated. Workaround: bypass `imklaunchagent` entirely — the LaunchAgent runs the binary, the binary's own `IMKServer(name:)` publishes the Mach service, host apps connect directly. **Do NOT declare `MachServices` in the LaunchAgent plist** (causes "switch-then-auto-skip" via IMKServer name conflict).

Apply Gate 1 first (no point debugging Gate 2 if Gate 1 fails). Walk the [Symptom → fix table](#symptom--fix-table) for specific cases. Full bundle skeleton in [Skeleton](#skeleton). All code blocks below are self-contained and copy-pasteable.

**During iteration**: never `killall cfprefsd` as a reset step — it wipes the user's `AppleEnabledInputSources` preference state, forcing them to re-Add the IME through System Settings UI and re-approve the third-party IME permission popup.

---

## TL;DR

A third-party IME on macOS 26 has to clear two independent gates. Each has its own set of rules; satisfying one tells you nothing about the other. The investigation order matters: solve Gate 1 first because Gate 2 isn't even meaningful until the picker shows your IME.

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

### LaunchAgent

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

Gate 1 makes the IME *appear* in the picker. Gate 2 is what makes it *type*. The two are independent: every Gate 1 fix can be in place, and the picker enumerates the IME, and the user can switch to it, and nothing happens when they type. This investigation kicked off after the v1.0.1 release shipped picker enumeration and we discovered that we'd never actually verified typing.

### 6. Discover the "switches but doesn't type" failure mode

The first concrete signal: the user installs the freshly-built bundle from the `.dmg`, adds it via System Settings, approves the third-party IME permission popup. Switches to the IME in WeChat. WeChat **crashes**:

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

The host app's TextInputUIMacHelper is dereferencing NULL right at the start of `_selectCurrentInputSource`. The offset `+ 84` says it's very early in the function — most likely a `TISCopy*` call returning nil for a property the helper unconditionally releases.

Diff vs. vChewing's Resources directory:

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

There's the problem: vChewing's `.icns` is 51 KB modern `ic13`; ours is 1.1 KB legacy `TOC ` (`il32`). `IconRef` returns NULL for the legacy single-resolution blob in macOS 26; host's TextInputUIMacHelper doesn't NULL-check.

Rebuild a proper iconset (see [Skeleton — `.icns`](#multi-resolution-icns)). 137 KB modern `ic12` `.icns`. Crash gone.

But typing still doesn't work — now it's "switches OK, no crash, but no characters appear when I type".

### 7. Locate the imklaunchagent refusal

`pgrep -fl Inputx` while a host app is in "Inputx is the active input source" state: nothing. Our binary was never launched. `IMKServer` never ran, no Mach service was published, host's IMK client has nothing to connect to.

Why? Capture broad log:

```bash
log show --last 60s --predicate 'process == "imklaunchagent" OR composedMessage CONTAINS "InputMethodConnectionName"'
```

Hit:

```
14:26:14.285 imklaunchagent[434] [com.apple.inputmethodkit:Server]
    Refusing connection name for bundle: unrecognized 'InputMethodConnectionName' value
```

Disassemble `imklaunchagent` (lives at `/System/Library/Frameworks/InputMethodKit.framework/Resources/imklaunchagent`, *not* in dyld_shared_cache) and the IMK framework (`ipsw dyld extract`'d) and `strings` it:

```bash
strings -a /System/Library/Frameworks/InputMethodKit.framework/Resources/imklaunchagent | grep -i 'connection\|inputmethod\|valid'
# isValidBundleIdentifier:
# connectionNameFor:
# .inputmethod.
# kInputMethodExecutablePathKey
# kInputMethodBundleIdentifierKey
# kInputMethodNeedSandboxExtensionKey
# kInputMethodIsNSExtensionKey

strings -a /tmp/dsc_extract/InputMethodKit | grep -i 'refusing\|allowedinput\|connection'
# Refusing connection name for bundle: invalid bundleIdentifier
# Refusing connection name for bundle: unrecognized 'InputMethodConnectionName' value
# _allowedInputMethodConnectionNames
# SCIM_1_Connection
# TCIM_1_Connection
# com.apple.inputmethod.JIM.IMK_Connection
# com.apple.inputmethod.Ainu.IMK_Connection
```

So there's an `_allowedInputMethodConnectionNames` method that decides which connection names IMK will accept. The hardcoded names visible in strings include Apple-internal IMEs only. Third-party bundles must be allowed via a different code path.

### 8. Match vChewing's bundle config bit-for-bit

vChewing is the modern third-party IME that we know works on this Mac. Diff its `Info.plist` keys against ours:

```
$ diff \
    <(plutil -p vChewing.app/Contents/Info.plist | grep -oE '"[A-Z][a-zA-Z]+"' | sort -u) \
    <(plutil -p MyIME.app/Contents/Info.plist  | grep -oE '"[A-Z][a-zA-Z]+"' | sort -u)
< "InputMethodServerDataSourceClass"
< "InputMethodSessionController"
```

vChewing has two `InputMethod*` keys we don't. Both undocumented (zero hits in Apple's developer.apple.com for either name in 2026). Both required by `imklaunchagent` apparently. Add them to `Info.plist`, point all four `InputMethodServer*` + `InputMethodSessionController` at the same Swift class.

Diff entitlements:

```
$ codesign -d --entitlements - vChewing.app  2>&1 | grep '<key>'
<key>com.apple.security.app-sandbox</key>
<key>com.apple.security.files.bookmarks.app-scope</key>
<key>com.apple.security.files.user-selected.read-write</key>
<key>com.apple.security.network.client</key>
<key>com.apple.security.temporary-exception.files.home-relative-path.read-only</key>
<key>com.apple.security.temporary-exception.files.home-relative-path.read-write</key>
<key>com.apple.security.temporary-exception.mach-register.global-name</key>
<key>com.apple.security.temporary-exception.shared-preference.read-only</key>
```

Eight entitlements; we had two. The two `home-relative-path` exceptions are vChewing-specific paths (importing Yahoo KeyKey dictionary and iCloud sync). The other six are generic IME requirements. Add them.

Also rule-out: notarize + staple the bundle. Confirm `spctl --assess --verbose=4 --type install` reports `source=Notarized Developer ID` (not `Unnotarized Developer ID`).

After all of the above is in place: `imklaunchagent` no longer logs `Refusing`. But our binary still isn't getting launched on switch. Same end result.

### 9. LaunchAgent workaround

User testing both vChewing and our IME side by side on the same machine: vChewing types fine, ours doesn't. `launchctl list | grep -iE 'inputmethod\.'` shows vChewing's launched IMK service but nothing for us. Whatever `imklaunchagent`'s opaque allow/refuse logic is, it accepts vChewing and silently refuses us, even with config that visibly matches vChewing's.

Bypass it. Run our binary manually:

```bash
"$HOME/Library/Input Methods/MyIME.app/Contents/MacOS/MyIME" &
launchctl print "user/$(id -u)" | grep myime_Connection
# 0x119bfb    U   A   com.example.inputmethod.myime_Connection
```

The `U A` flag shows the binary self-published the Mach service. Switch to the IME in a text field, type — it works. So the binary, when running, is fully functional. The only piece missing is the launch.

Solution: ship a user-level LaunchAgent (`~/Library/LaunchAgents/<bundle-id>.plist`) with `RunAtLoad=true` and `KeepAlive`. The binary stays running across logins; the IMK Mach service is permanently published; host apps connect directly; `imklaunchagent`'s decision is irrelevant.

Two pitfalls cost an hour each:

- **Don't add `MachServices` to the LaunchAgent plist.** Launchd then claims the Mach service name; when the binary tries to `IMKServer(name:)` self-register, the names collide. Flag flips from `U A` to `M D`. The IME goes into a "switches and immediately auto-skips to the next" failure mode. Remove the `MachServices` key; let the binary self-register.
- **Don't `killall cfprefsd` for any purpose during this debugging.** `cfprefsd` holds `AppleEnabledInputSources` in RAM; killing it wipes the user's enabled-IME list. Symptom: the IME is in System Settings as "installed", but does not appear in the keyboard menu. The user has to re-Add it via System Settings UI to repopulate (and re-approve the permission popup). Cost us four iterations in the original session.

The LaunchAgent's `ProgramArguments` must contain an absolute path; substitute via your install script (see [Skeleton — `install.sh`](#installsh) and [Skeleton — `.pkg` postinstall](#pkg-postinstall)).

After the LaunchAgent loads + the user adds the IME via System Settings: typing works end-to-end. That's the moment 12 hours of investigation finally yields one character.

The proper fix is to figure out *why* `imklaunchagent` refuses our bundle in the first place — we never isolated it. Both vChewing's and ours satisfy every visible check (bundle id pattern, 4 IMK keys, 6 entitlements, signing, runtime). Something else gates it. If you isolate it, please file an issue on this article and we'll update.

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
  - [`mac/Sources/main.swift`](https://github.com/goliajp/inputx/blob/develop/mac/Sources/main.swift)
  - [`mac/install.sh`](https://github.com/goliajp/inputx/blob/develop/mac/install.sh) — Gate-1 install + Gate-2 LaunchAgent bootstrap
  - [`mac/pkg/scripts/postinstall`](https://github.com/goliajp/inputx/blob/develop/mac/pkg/scripts/postinstall) — same as above, .pkg-flavored
- **[vChewing](https://github.com/vChewing/vChewing-macOS)** — Taiwanese Bopomofo IME. Used as the reference working bundle for diff in §1.
- **[gureum](https://github.com/gureum/gureum)** — Korean IME, modern Swift.
- **[fcitx5-macos](https://github.com/fcitx-contrib/fcitx5-macos)** — general IME framework, includes Chinese/Japanese/Korean/Vietnamese.
- **[InputMethodKit Release Notes (2007)](https://developer.apple.com/library/archive/releasenotes/Cocoa/RN-InputMethodKit/index.html)** — the only Apple doc, still useful for IMK fundamentals.
- **[ipsw](https://blacktop.github.io/ipsw/)** — `dyld_shared_cache` extraction + Mach-O disassembly.

This article is on GitHub at [goliajp/inputx/docs/macos-ime-recipe-2026.md](https://github.com/goliajp/inputx/blob/develop/docs/macos-ime-recipe-2026.md). Issues and PRs welcome.
