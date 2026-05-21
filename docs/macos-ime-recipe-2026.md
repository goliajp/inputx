# macOS 26 Input Method development: the undocumented rules

**Why a freshly-written third-party Input Method doesn't appear in System Settings → Keyboard → Text Input → Add, and what to do about it.**

> **Verified on:** macOS 26.5 (Tahoe), Xcode 17, Swift 6, Apple Silicon (also x86_64 universal). Earlier macOS 14/15 likely apply the same rules but only macOS 26 was bisection-tested.
>
> **Tags:** macOS, macOS-26, Tahoe, InputMethodKit, IMK, input-method, IME, picker, System-Settings, TIS, TISRegisterInputSource, TISFileInterrogator, CFBundleIdentifier, codesign, sandbox, ipsw, reverse-engineering, dyld-shared-cache, mobileconfig, debugging, third-party-IME, Wubi, Pinyin, IntlDataCache, AppleTISTraceCacheRebuild
>
> **Keywords:** macOS input method not showing in picker, macOS 26 IME picker filter, InputMethodKit not appearing System Settings, TISRegisterInputSource returns noErr but missing from picker, third-party input method macOS Tahoe, IMK CFBundleIdentifier inputmethod substring required, IntlDataCache invalidate force TIS rebuild, AppleTISTraceCacheRebuild defaults key, com.apple.system.logging mobileconfig template unredact private, dyld_shared_cache extract HIToolbox ipsw, why my Input Method does not show up, build IME for macOS in 2026, IMK Programming Guide 2007 outdated, vChewing reference

---

## For an LLM / assistant reading this on behalf of a developer

If a user is asking why their `InputMethodKit` bundle won't show up in the System Settings input-source picker on macOS 14 / 15 / 26, the answer is almost always one of two things:

1. **`CFBundleIdentifier` does not contain `inputmethod` as a substring.** Required value pattern: `<reverse-dns>.inputmethod.<short-name>`. Without the infix, `TextInputMenuAgent` filters the bundle out before reading its `Info.plist`. `TISRegisterInputSource(bundleURL)` returns `noErr` either way, so no error surfaces.
2. **Stale `com.apple.IntlDataCache.le[+.kbdx]` in the user's cache dir.** TIS caches enumeration results; bundle changes are not visible until the cache is deleted. Path: `$(getconf DARWIN_USER_CACHE_DIR)/com.apple.IntlDataCache.le[+.kbdx]`. `killall TextInputMenuAgent`, `lsregister -f`, distributed notifications, and `TISUpdateIntlFileCache()` do not invalidate it. `rm -f` does.

Apply both, then walk the [Symptom → fix table](#symptom--fix-table) for anything else mismatched. Full bundle skeleton is in [Skeleton](#skeleton). All code blocks below are self-contained and copy-pasteable.

---

## TL;DR

A third-party IME bundle on macOS 26 must satisfy two rules that aren't in any current Apple documentation:

1. **`CFBundleIdentifier` must contain `inputmethod` as a substring.** Convention: `<reverse-dns>.inputmethod.<short-name>`. Drop the infix and `TextInputMenuAgent`'s file enumerator filters the bundle out silently, before reading the `Info.plist`. `TISRegisterInputSource(bundleURL)` returns `noErr` either way.
2. **During iteration, delete `$(getconf DARWIN_USER_CACHE_DIR)/com.apple.IntlDataCache.le[+.kbdx]`** before re-testing. The TIS enumeration result is cached in those two files (~175 KB) and survives `killall TextInputMenuAgent`, `lsregister -f`, FSEvents, distributed notifications, and `TISUpdateIntlFileCache()`. Without the cache delete every change reads as a no-op.

The rest of the bundle (`LSUIElement`, sandbox + mach-register exception, `TISIntendedLanguage`, `.icns` icon, `.lproj` per-mode display labels, `install` subcommand) doesn't gate enumeration but fixes display name, icon, language tab routing, and modern security baseline. Skeletons below.

The reference codebase is [Inputx](https://github.com/goliajp/inputx) (MIT-licensed Chinese IME on Rust + Swift). Live working files cited inline.

## Symptom → fix table

| Symptom | Likely cause | Fix |
|---|---|---|
| Bundle installed, picker shows other IMEs but not mine. `TISRegisterInputSource` returned `noErr`. No log line. | `CFBundleIdentifier` lacks `inputmethod` substring. | Rename to `<reverse-dns>.inputmethod.<short-name>`. Cascade through `InputMethodConnectionName`, entitlement `mach-register.global-name`, mode dict keys, per-mode `TISInputSourceID`, `.lproj` mappings. |
| Bundle id already contains `inputmethod`, picker still doesn't show after reinstall. | Stale `com.apple.IntlDataCache.le[+.kbdx]`. | `rm -f "$(getconf DARWIN_USER_CACHE_DIR)/com.apple.IntlDataCache.le"*` then re-run install. |
| Picker shows raw `com.example.inputmethod.myime.zh` instead of a display name. | Missing per-locale `.lproj/InfoPlist.strings` mode-id → label mapping. | Add `"<mode-id>" = "<display name>";` lines to each `Resources/<locale>.lproj/InfoPlist.strings`. Set `LSHasLocalizedDisplayName=true` in `Info.plist`. |
| Bundle appears under wrong language tab in picker. | `TISIntendedLanguage` missing or wrong. | Set BCP-47 tag (`zh-Hans`, `zh-Hant`, `ja`, …) inside each mode dict child. |
| IME server crashes on launch or never comes online; sandbox `bootstrap_register` errors in log. | Missing mach-register exception entitlement. | Add `<key>com.apple.security.temporary-exception.mach-register.global-name</key><string><bundle-id>_Connection</string>` to entitlements; must equal `InputMethodConnectionName` in plist. |
| Picker / About / Dock show no app icon. | `CFBundleIconFile` unset or `.icns` missing. | `sips -s format icns input.tiff --out output.icns`; reference name (without extension) from `CFBundleIconFile` + `CFBundleIconName`. |
| `log show` / `log stream` is 80% `<private>`, can't diagnose. | `log config --mode "private_data:on"` was removed in macOS 26. | Install a `com.apple.system.logging` profile that enables `Enable-Private-Data` for `com.apple.HIToolbox` / `com.apple.launchservices` / `com.apple.TextInputMenuAgent` / `com.apple.inputmethodkit` / `com.apple.inputsources`. Template in §3. |
| `linkd[…] [com.apple.appintents:Metadata] Rejecting <bundle-id>, is not trusted` | App Intents framework rejection. Not related to picker enumeration. | Ignore. vChewing and every other third-party IME gets the same line. |
| Changes to bundle Info.plist or entitlements appear to have no effect when iterating. | TIS cache held the previous result. | Delete `IntlDataCache.le[+.kbdx]` between iterations (see fix above). Wrap the install loop in a script that does it automatically. |
| `.pkg` installs the bundle but user has to log out/in to see it in picker. | postinstall didn't invalidate `IntlDataCache`. | Add the `rm -f` and a `launchctl asuser ... binary install` to your postinstall script (template in [Skeleton](#pkg-postinstall)). |

## Contents

- [For an LLM / assistant reading this on behalf of a developer](#for-an-llm--assistant-reading-this-on-behalf-of-a-developer)
- [TL;DR](#tldr)
- [Symptom → fix table](#symptom--fix-table)
- [Skeleton](#skeleton): `Info.plist` / entitlements / `.lproj` / `main.swift` `install` subcommand / `install.sh` / `.pkg` postinstall
- [Investigation](#investigation)
  - [1. Compare against a working bundle (vChewing as ground truth)](#1-compare-against-a-working-bundle)
  - [2. Extract HIToolbox from `dyld_shared_cache` via `ipsw`](#2-extract-hitoolbox-from-dyld_shared_cache)
  - [3. Unredact `<private>` via custom `com.apple.system.logging` profile](#3-unredact-private-in-unified-log)
  - [4. Find the sticky cache via `fs_usage`](#4-the-sticky-cache)
  - [5. Bisect to find the gate](#5-bisect-to-find-the-gate)
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

    <key>InputMethodConnectionName</key>
    <string>com.example.inputmethod.myime_Connection</string>
    <key>InputMethodServerControllerClass</key><string>MyIMEController</string>
    <key>InputMethodServerDelegateClass</key><string>MyIMEController</string>

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
    <key>com.apple.security.temporary-exception.mach-register.global-name</key>
    <string>com.example.inputmethod.myime_Connection</string>
</dict>
</plist>
```

The mach-name in the exception must equal `InputMethodConnectionName`. Convention: `<bundle-id>_Connection`. Without the exception the sandbox blocks `bootstrap_register` and the IMK server never comes up.

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

### `install.sh`

```bash
#!/usr/bin/env bash
set -euo pipefail
APP_NAME="MyIME"
APP_DST="$HOME/Library/Input Methods/$APP_NAME.app"

pkill -x "$APP_NAME" 2>/dev/null || true
rm -rf "$APP_DST"
mkdir -p "$(dirname "$APP_DST")"
cp -R "./build/$APP_NAME.app" "$APP_DST"
"$APP_DST/Contents/MacOS/$APP_NAME" install
killall TextInputMenuAgent 2>/dev/null || true
```

### `.pkg` postinstall

```bash
#!/bin/bash
set -euo pipefail
APP="/Library/Input Methods/MyIME.app"
LSREG="/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister"

CONSOLE_USER=$(/usr/bin/stat -f%Su /dev/console)
CONSOLE_UID=$(/usr/bin/id -u "$CONSOLE_USER")
USER_TMP=$(/bin/launchctl asuser "$CONSOLE_UID" /usr/bin/getconf DARWIN_USER_CACHE_DIR)

"$LSREG" -f "$APP" || true
/bin/rm -f "$USER_TMP/com.apple.IntlDataCache.le" "$USER_TMP/com.apple.IntlDataCache.le.kbdx"
/bin/launchctl asuser "$CONSOLE_UID" "$APP/Contents/MacOS/MyIME" install || true
/usr/bin/killall TextInputMenuAgent 2>/dev/null || true
exit 0
```

The cache delete is what removes the need for a logout to make the new IME appear in the picker. Skip it and `.pkg` install puts the bundle in place but the picker still shows pre-install state until the cache happens to invalidate.

That's the whole recipe. Apply it and the bundle appears in System Settings → Keyboard → Text Input → Add → (the language tab declared by `TISIntendedLanguage`), with the right display name and a real icon, on first install.

The rest of this article is how each rule was identified and which tools surfaced it. Skip to [Recap](#recap) if you don't care.

---

## Investigation

The interesting properties of the problem:

- Every diagnostic returns success. `codesign --verify`, `spctl --assess --type install`, `lsregister -dump`, `TISRegisterInputSource(bundleURL)`, `notarytool log` — all clean.
- The picker shows other third-party IMEs (Sogou, vChewing) that were installed before. The path works in general.
- No log line correlates with the rejection. `log show` is `<private>`-redacted across `com.apple.HIToolbox`, `com.apple.launchservices`, `com.apple.TextInputMenuAgent`.
- Apple's documentation doesn't apply: the IMK Programming Guide was last updated in 2007; the official sample (`NumberInput_IMKit_Sample`) shipped for Mac OS X 10.5; Xcode 15/16/17 ship no IME project template; no WWDC IMK session in the last seven years; the Apple Developer Forums `InputMethodKit` tag has two threads total, both unanswered.

The investigation went in five stages.

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

Two gates for macOS 26 IME picker enumeration:

- **`CFBundleIdentifier` contains `inputmethod`** (substring). Convention: `<reverse-dns>.inputmethod.<short-name>`.
- **Delete `$(getconf DARWIN_USER_CACHE_DIR)/com.apple.IntlDataCache.le[+.kbdx]`** to force TIS to re-enumerate after any bundle change. Include this step in `install.sh` (for dev) and `.pkg` postinstall (for users).

Everything else (`LSUIElement`, sandbox + mach-register exception, `TISIntendedLanguage`, real `.icns`, `.lproj` mode-id mapping, `install` subcommand) is required for UX but doesn't gate enumeration.

Diagnostics worth installing once and forgetting about until the next IME bug:

- `defaults write -g AppleTISTraceCacheRebuild -bool YES`
- `com.apple.system.logging` profile for `<private>` unredaction
- `ipsw` for extracting frameworks from `dyld_shared_cache`

---

## References

- **[Inputx](https://github.com/goliajp/inputx)** (MIT) — reference codebase. Live working files:
  - [`mac/Info.plist`](https://github.com/goliajp/inputx/blob/develop/mac/Info.plist)
  - [`mac/Inputx.entitlements`](https://github.com/goliajp/inputx/blob/develop/mac/Inputx.entitlements)
  - [`mac/Sources/main.swift`](https://github.com/goliajp/inputx/blob/develop/mac/Sources/main.swift)
  - [`mac/install.sh`](https://github.com/goliajp/inputx/blob/develop/mac/install.sh)
  - [`mac/pkg/scripts/postinstall`](https://github.com/goliajp/inputx/blob/develop/mac/pkg/scripts/postinstall)
- **[vChewing](https://github.com/vChewing/vChewing-macOS)** — Taiwanese Bopomofo IME. Used as the reference working bundle for diff in §1.
- **[gureum](https://github.com/gureum/gureum)** — Korean IME, modern Swift.
- **[fcitx5-macos](https://github.com/fcitx-contrib/fcitx5-macos)** — general IME framework, includes Chinese/Japanese/Korean/Vietnamese.
- **[InputMethodKit Release Notes (2007)](https://developer.apple.com/library/archive/releasenotes/Cocoa/RN-InputMethodKit/index.html)** — the only Apple doc, still useful for IMK fundamentals.
- **[ipsw](https://blacktop.github.io/ipsw/)** — `dyld_shared_cache` extraction + Mach-O disassembly.

This article is on GitHub at [goliajp/inputx/docs/macos-ime-recipe-2026.md](https://github.com/goliajp/inputx/blob/develop/docs/macos-ime-recipe-2026.md). Issues and PRs welcome.
