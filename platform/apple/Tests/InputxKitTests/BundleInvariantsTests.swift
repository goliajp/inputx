import XCTest

/// Static-config + source-pattern invariants that make Inputx survive as
/// a third-party IME on macOS 26 and not crash host apps (WeChat, the
/// canary that taught us each of these the hard way).
///
/// Each test lines up against a past incident — break the invariant and
/// the corresponding symptom reappears. See feedback memory
/// [[testable-invariants-only]] for why install/runtime audits are NOT
/// in this file (they live in scripts).
final class BundleInvariantsTests: XCTestCase {

    /// Walk up from this test file's path to the repo root. `#filePath`
    /// is resolved at compile time to an absolute path, so this works
    /// regardless of the test's CWD at runtime.
    private var projectRoot: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()   // InputxKitTests/
            .deletingLastPathComponent()   // Tests/
            .deletingLastPathComponent()   // apple/
            .deletingLastPathComponent()   // platform/
            .deletingLastPathComponent()   // <repo root>/
    }

    private func readPlist(_ relative: String) throws -> [String: Any] {
        let url = projectRoot.appendingPathComponent(relative)
        let data = try Data(contentsOf: url)
        guard let plist = try PropertyListSerialization.propertyList(
            from: data, options: [], format: nil
        ) as? [String: Any] else {
            throw XCTSkip("not a dictionary plist: \(relative)")
        }
        return plist
    }

    private func readSource(_ relative: String) throws -> String {
        let url = projectRoot.appendingPathComponent(relative)
        return try String(contentsOf: url, encoding: .utf8)
    }

    private func macSwiftSources() throws -> [URL] {
        let dir = projectRoot.appendingPathComponent("mac/Sources")
        let urls = try FileManager.default.contentsOfDirectory(
            at: dir, includingPropertiesForKeys: nil
        )
        return urls.filter { $0.pathExtension == "swift" }
    }

    // MARK: - A. Static config invariants ------------------------------------

    /// A1. `CFBundleIdentifier` must contain `inputmethod` — macOS 26's
    /// input-source picker (Gate 1) only enumerates third-party IMEs whose
    /// bundle id has that reverse-DNS infix. Drop it and Inputx silently
    /// disappears from System Settings.
    func testA1_BundleIdContainsInputmethod() throws {
        let plist = try readPlist("mac/Info.plist")
        let bundleId = (plist["CFBundleIdentifier"] as? String) ?? ""
        XCTAssertTrue(
            bundleId.contains("inputmethod"),
            "CFBundleIdentifier '\(bundleId)' missing 'inputmethod' infix (macOS 26 Gate 1)"
        )
    }

    /// A2. The 4 IMK controller-class keys must all be present — missing any
    /// causes imklaunchagent to silently refuse spawning Inputx on input-
    /// source switch (macOS 26 Gate 2). Plus `InputMethodConnectionName`,
    /// without which the IMKServer can't claim a Mach name.
    func testA2_InfoPlistHasImkControllerKeys() throws {
        let plist = try readPlist("mac/Info.plist")
        let required = [
            "InputMethodServerControllerClass",
            "InputMethodServerDelegateClass",
            "InputMethodServerDataSourceClass",
            "InputMethodSessionController",
            "InputMethodConnectionName",
        ]
        for key in required {
            XCTAssertNotNil(
                plist[key] as? String,
                "Info.plist missing IMK key: \(key)"
            )
        }
    }

    /// A3. The plist controller class string must match the Swift
    /// `@objc(...)` attribute on the IMKInputController subclass — IMKit
    /// resolves the class via the Objective-C runtime by that exact name.
    /// Mismatch silently drops the bundle from the picker.
    func testA3_ControllerClassMatchesObjcAttribute() throws {
        let plist = try readPlist("mac/Info.plist")
        let plistClass = (plist["InputMethodServerControllerClass"] as? String) ?? ""
        let source = try readSource("mac/Sources/IMEController.swift")
        let regex = try NSRegularExpression(
            pattern: #"@objc\(([A-Za-z_][A-Za-z0-9_]*)\)"#
        )
        let range = NSRange(source.startIndex..., in: source)
        guard let match = regex.firstMatch(in: source, range: range),
              let r = Range(match.range(at: 1), in: source) else {
            XCTFail("no @objc(...) attribute found in IMEController.swift")
            return
        }
        let objcName = String(source[r])
        XCTAssertEqual(
            plistClass, objcName,
            "InputMethodServerControllerClass '\(plistClass)' != @objc('\(objcName)')"
        )
    }

    /// A4. `ComponentInputModeDict.tsInputModeListKey` must list at least
    /// one sub-mode — empty list = user has no checkable entry under the
    /// umbrella source and the language tab stays unusable.
    func testA4_InputModeListIsNonEmpty() throws {
        let plist = try readPlist("mac/Info.plist")
        let dict = (plist["ComponentInputModeDict"] as? [String: Any]) ?? [:]
        let list = (dict["tsInputModeListKey"] as? [String: Any]) ?? [:]
        XCTAssertGreaterThanOrEqual(
            list.count, 1,
            "ComponentInputModeDict.tsInputModeListKey must have ≥1 mode"
        )
    }

    /// A5. The 6 entitlements that let an IMK Mach service register under
    /// the macOS 26 sandbox. The load-bearing one is
    /// `temporary-exception.mach-register.global-name`; sandbox blocks
    /// `bootstrap_register` without it.
    func testA5_EntitlementsHasRequiredKeys() throws {
        let plist = try readPlist("mac/Inputx.entitlements")
        let required = [
            "com.apple.security.app-sandbox",
            "com.apple.security.files.bookmarks.app-scope",
            "com.apple.security.files.user-selected.read-write",
            "com.apple.security.network.client",
            "com.apple.security.temporary-exception.mach-register.global-name",
            "com.apple.security.temporary-exception.shared-preference.read-only",
        ]
        for key in required {
            XCTAssertNotNil(
                plist[key],
                "Inputx.entitlements missing required key: \(key)"
            )
        }
    }

    /// A5b. The mach-register entitlement value MUST equal Info.plist's
    /// `InputMethodConnectionName` — they're the same Mach name and the
    /// sandbox compares them character-for-character at register time.
    /// Drift between the two = service never registers = picker shows
    /// Inputx but typing produces nothing.
    func testA5b_MachRegisterMatchesConnectionName() throws {
        let info = try readPlist("mac/Info.plist")
        let ent = try readPlist("mac/Inputx.entitlements")
        let connName = info["InputMethodConnectionName"] as? String
        let machName = ent["com.apple.security.temporary-exception.mach-register.global-name"] as? String
        XCTAssertEqual(
            connName, machName,
            "InputMethodConnectionName ('\(connName ?? "nil")') must equal mach-register entitlement ('\(machName ?? "nil")')"
        )
    }

    /// A6. `inputx_app_icon.icns` must be a real multi-resolution .icns —
    /// 16/32/64/128/256/512 + @2x built via `iconutil -c icns iconset/`.
    /// A 1-resolution legacy `il32` blob (e.g. from `sips -s format icns`)
    /// is only a few KB and makes host apps SIGTRAP on input-source switch
    /// via IconRef → CFRelease(NULL). 50KB threshold separates the two
    /// empirically; our current build is ~134KB.
    func testA6_IconIsMultiResolution() throws {
        let url = projectRoot.appendingPathComponent(
            "mac/Resources/inputx_app_icon.icns"
        )
        let attrs = try FileManager.default.attributesOfItem(atPath: url.path)
        let size = (attrs[.size] as? Int) ?? 0
        XCTAssertGreaterThan(
            size, 50_000,
            ".icns is \(size) bytes — likely single-resolution; rebuild via iconutil -c icns iconset/"
        )
    }

    /// A7. Each .lproj must carry an `InfoPlist.strings` that maps the
    /// mode ID to a friendly label. Without it the HUD picker / language
    /// tab falls back to the raw bundle ID `jp.golia.inputmethod.wubi.zh`
    /// instead of "Inputx 五笔". Content assertion checks the mode-id
    /// key is actually present in each file — empty .strings = same
    /// symptom as missing file.
    func testA7_AllLprojInfoPlistStringsHaveModeLabel() throws {
        let info = try readPlist("mac/Info.plist")
        let modeDict = (info["ComponentInputModeDict"] as? [String: Any]) ?? [:]
        let modes = (modeDict["tsInputModeListKey"] as? [String: Any]) ?? [:]
        guard let modeKey = modes.keys.first else {
            XCTFail("no mode in tsInputModeListKey to check for")
            return
        }
        let locales = ["en", "zh-Hans", "zh-Hant", "ja"]
        for locale in locales {
            let url = projectRoot.appendingPathComponent(
                "mac/Resources/\(locale).lproj/InfoPlist.strings"
            )
            XCTAssertTrue(
                FileManager.default.fileExists(atPath: url.path),
                "missing \(locale).lproj/InfoPlist.strings"
            )
            // .strings on disk may be UTF-8 or UTF-16; try both.
            let data = (try? Data(contentsOf: url)) ?? Data()
            let content =
                String(data: data, encoding: .utf8)
                ?? String(data: data, encoding: .utf16)
                ?? ""
            XCTAssertTrue(
                content.contains(modeKey),
                "\(locale).lproj/InfoPlist.strings has no entry for mode '\(modeKey)' — picker will show naked bundle id"
            )
        }
    }

    // MARK: - B. Source-pattern invariants -----------------------------------

    /// B1. No `TIS(Copy|Create)*().takeRetainedValue()` or `...()!.takeRetainedValue()`
    /// — both crash on NULL. The safe pattern is `?.takeRetainedValue()`
    /// with an `if let` binding. The WeChat CFRelease(NULL) crash chain
    /// (2026-05-21) was kicked off by exactly this antipattern in
    /// `CandidatePanel.init` poisoning `_currentKeyboardLayout`.
    func testB1_NoForceUnwrapOnTisApis() throws {
        // Matches `TIS(Copy|Create)<Name>(...).takeRetainedValue()` OR
        //         `TIS(Copy|Create)<Name>(...)!.takeRetainedValue()`
        // but NOT the safe `TIS(Copy|Create)<Name>(...)?.takeRetainedValue()`
        // — in the safe form, `?` sits between `)` and `.`, breaking this
        // regex (which only allows nothing or `!` between them).
        let pattern = #"TIS(?:Copy|Create)[A-Za-z]+\([^)]*\)!?\.takeRetainedValue\(\)"#
        let regex = try NSRegularExpression(pattern: pattern)
        for url in try macSwiftSources() {
            let source = try String(contentsOf: url, encoding: .utf8)
            let range = NSRange(source.startIndex..., in: source)
            let matches = regex.matches(in: source, range: range)
            if !matches.isEmpty {
                let snippets = matches.compactMap { m -> String? in
                    Range(m.range, in: source).map { String(source[$0]) }
                }
                XCTFail(
                    "\(url.lastPathComponent): unsafe TIS API force-unwrap "
                    + "found — use `?.takeRetainedValue()` with `if let`: \(snippets)"
                )
            }
        }
    }

    /// B2. `IMEController.recognizedEvents` MUST include `flagsChanged` —
    /// without it, modifier-only events (pure shift presses/releases) never
    /// reach `handle()`, so shift single-click detection silently dies and
    /// the CJK↔EN toggle stops working. The default IMK recognizedEvents
    /// is keyDown-only.
    func testB2_RecognizedEventsIncludesFlagsChanged() throws {
        let source = try readSource("mac/Sources/IMEController.swift")
        XCTAssertTrue(
            source.contains("recognizedEvents"),
            "IMEController.swift must override recognizedEvents()"
        )
        XCTAssertTrue(
            source.contains("flagsChanged"),
            "IMEController.swift recognizedEvents must include flagsChanged for shift detection"
        )
    }
}
