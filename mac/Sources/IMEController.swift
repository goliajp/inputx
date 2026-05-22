import Cocoa
import InputMethodKit
import InputxKit

extension Notification.Name {
    /// Posted whenever any `InputxController` toggles between CJK and EN.
    /// `userInfo["mode"]` is a `UInt8` matching `InputxInputMode.rawValue`.
    /// Used by `MenubarSettings` to refresh its status-item indicator.
    static let inputxInputModeChanged = Notification.Name("InputxInputModeChanged")
}

/// IMKit input controller — one instance per client (text view / editor).
///
/// **`@objc(InputxController)` is load-bearing.** IMKit reads the class
/// name from `Info.plist`'s `InputMethodServerControllerClass` string
/// and resolves it through the Objective-C runtime. Without an explicit
/// `@objc(...)` name, Swift mangles the symbol to
/// `<ModuleName>.InputxController`, IMKit can't find the class, and the
/// System Settings input-source picker silently drops the bundle from
/// its enumeration — the IME "doesn't exist" from the user's POV.
///
/// Lifecycle each keystroke:
///   1. Map NSEvent → (codepoint, modifiers).
///   2. Number-key shortcut path: if the candidate panel is visible and a
///      1-9 digit comes in, commit that candidate directly (no engine call).
///   3. Symbol path: in zh mode, route ASCII punctuation through the locale
///      helpers (CJK punct + smart quotes + full-width) before insertion.
///   4. Engine path: feed (cp, mods) into the Rust session. Drain any
///      auto-commit / force-commit text. Refresh marked-text preedit +
///      candidate panel.
@objc(InputxController)
final class InputxController: IMKInputController {
    private let session = InputxSession()
    private var candidatePanel: CandidatePanel?
    /// Detects pure shift single-clicks (no other key in between) to
    /// toggle `InputxInputMode` between `.cjk` and `.en`. See
    /// `InputxShiftSingleClickDetector` for the state machine.
    private let shiftDetector = InputxShiftSingleClickDetector()

    override init!(server: IMKServer!, delegate: Any!, client inputClient: Any!) {
        super.init(server: server, delegate: delegate, client: inputClient)
        applySettingsToSession()
        // Custom CandidatePanel (no IMKServer needed — see CandidatePanel.swift
        // for why we dropped IMKCandidates in favor of a custom NSWindow).
        _ = server
        self.candidatePanel = CandidatePanel()
        // Pay the FST / 简拼-index cold-start cost up front so the first
        // measured keystroke doesn't take ~1-2 s.
        session.warmup()
        // Re-hydrate user-learning state from the on-disk JSON store.
        inputxL0Storage.load(into: session)
        // Process-global rare-CJK toggle reads from prefs at startup.
        InputxRareChars.enabled = inputxSettings.showRareChars

        // Listen for live settings changes (broadcast by MenubarSettings
        // and SettingsWindow). Without this, the user has to switch input
        // sources out and back to trigger `activateServer` before a
        // freshly toggled JP-enable / engine-mode / policy / locale flag
        // actually reaches the running engine.
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleSettingsChanged),
            name: .inputxSettingsChanged,
            object: nil
        )
    }

    deinit {
        NotificationCenter.default.removeObserver(self)
    }

    @objc private func handleSettingsChanged() {
        applySettingsToSession()
        InputxRareChars.enabled = inputxSettings.showRareChars
    }

    // MARK: - IMKit overrides ------------------------------------------------

    override func activateServer(_ sender: Any!) {
        super.activateServer(sender)
        // Re-pick up any settings changes made while another client was active.
        applySettingsToSession()
        InputxRareChars.enabled = inputxSettings.showRareChars
    }

    override func deactivateServer(_ sender: Any!) {
        // Client switched away while composing — drop in-flight state rather
        // than auto-commit into a textfield the user just left.
        session.clear()
        candidatePanel?.hide()
        clearMarkedText(client: sender)
        // A shift held across deactivation would otherwise leave the
        // detector armed forever; reset.
        shiftDetector.reset()
        // Best-effort persist of L0 state; cheap (atomic JSON write).
        inputxL0Storage.save(from: session)
        super.deactivateServer(sender)
    }

    /// Expand IMKit's default keyDown-only event set to also include
    /// `flagsChanged`, so we can observe pure shift presses/releases.
    /// Without this override, modifier-only events never reach `handle`.
    override func recognizedEvents(_ sender: Any!) -> Int {
        return Int(
            NSEvent.EventTypeMask.keyDown.rawValue
                | NSEvent.EventTypeMask.flagsChanged.rawValue
        )
    }

    override func handle(_ event: NSEvent!, client sender: Any!) -> Bool {
        guard let event = event else { return false }

        if event.type == .flagsChanged {
            return handleFlagsChanged(event: event, client: sender)
        }
        guard event.type == .keyDown else { return false }
        // Any keyDown disarms the shift detector — shift wasn't alone.
        shiftDetector.observeKeyDown()

        // EN mode: IME steps aside. Host receives the raw ASCII keystroke
        // (including return / backspace / cmd-combos) directly. We still
        // observed shiftDown above so a subsequent single-shift toggle is
        // detectable; everything else is a pure passthrough.
        if session.inputMode == .en {
            return false
        }

        guard let chars = event.charactersIgnoringModifiers,
              let firstScalar = chars.unicodeScalars.first
        else { return false }
        let codepoint = firstScalar.value

        // Apple PUA range for special keys (arrows, function keys). When
        // the panel is visible, ↑ / ↓ / ← / → drive the panel; other PUA
        // keys still pass through to the host. When the panel is hidden,
        // all PUA passes through.
        if (0xF700...0xF8FF).contains(codepoint) {
            if let panel = candidatePanel, panel.isVisible {
                switch codepoint {
                case 0xF700: // up arrow
                    _ = panel.moveSelectionUp()
                    return true
                case 0xF701: // down arrow
                    _ = panel.moveSelectionDown()
                    return true
                case 0xF702: // left arrow → previous page
                    _ = panel.prevPage()
                    return true
                case 0xF703: // right arrow → next page
                    _ = panel.nextPage()
                    return true
                default:
                    break
                }
            }
            return false
        }

        // ---- Panel pagination via Tab / + / - -----------------------------
        // Same effect as ← / → arrows, exposed under the keys the user
        // already has muscle memory for. Tab = next page (Shift+Tab =
        // previous), + = next, - = previous. Intercepts BEFORE Path B
        // (locale punct) so the `+`/`-` keystrokes never reach the host
        // when the panel is up.
        if let panel = candidatePanel, panel.isVisible {
            let shifted = event.modifierFlags.contains(.shift)
            switch codepoint {
            case 0x09: // Tab
                if shifted { _ = panel.prevPage() } else { _ = panel.nextPage() }
                return true
            case 0x2B: // '+'
                _ = panel.nextPage()
                return true
            case 0x2D: // '-'
                _ = panel.prevPage()
                return true
            default:
                break
            }
        }

        // ---- Path A0: Space → commit highlighted (not just #0) -----------
        // When the panel is visible and ↑/↓ has moved the highlight off #0,
        // Space commits the *highlighted* candidate. If highlight is on #0
        // (panel just opened), this matches the legacy "Space = commit #0"
        // semantic. Falls through if not composing.
        if codepoint == 0x20,
           let panel = candidatePanel, panel.isVisible,
           let idx = panel.selectedAbsoluteIndex(),
           idx > 0
        {
            let bufferBefore = session.preedit ?? ""
            let candsBefore = panel.current
            if let committed = session.commit(at: idx), !committed.isEmpty {
                commitText(committed, to: sender)
                PolishLog.recordIfMiss(
                    buffer: bufferBefore,
                    candidates: candsBefore,
                    pickedIdx: idx,
                    pickedWord: committed,
                    engineMode: inputxSettings.engineMode.rawValue,
                    japaneseEnabled: inputxSettings.japaneseEnabled
                )
            }
            panel.hide()
            updatePreedit(client: sender)
            return true
        }

        // ---- Path A: number-key candidate commit ---------------------------
        // When the panel is up, 1-9 + 0 picks the corresponding candidate
        // (0 → 10th slot) without touching the engine state machine.
        if let panel = candidatePanel, panel.isVisible,
           let idx = panel.candidateIndex(forNumberKey: codepoint) {
            let bufferBefore = session.preedit ?? ""
            let candsBefore = panel.current
            if let committed = session.commit(at: idx), !committed.isEmpty {
                commitText(committed, to: sender)
                // Telemetry: log #0 != #picked as a polish-corpus signal.
                PolishLog.recordIfMiss(
                    buffer: bufferBefore,
                    candidates: candsBefore,
                    pickedIdx: idx,
                    pickedWord: committed,
                    engineMode: inputxSettings.engineMode.rawValue,
                    japaneseEnabled: inputxSettings.japaneseEnabled
                )
            }
            panel.hide()
            updatePreedit(client: sender)
            return true
        }

        // ---- Path B: ASCII punct / symbol -- one consistent flow -------
        //
        // Behavior contract (also defends against two user-reported bugs):
        //   (1) "houxuan + ," ghost-candidate. Previously, the engine's
        //       default-arm pushed 候选 to `pending_commit` then escaped,
        //       and returned consumed=false. The host then received the
        //       raw `,` via IMK default routing, but IMEController never
        //       drained `pending_commit` on the consumed=false branch —
        //       so 候选 hung around as a ghost that re-appeared on the
        //       next keystroke. Fix: on punct mid-compose, *force-commit
        //       the top candidate via the IME path* (deterministic),
        //       hide the panel, then process the punct as if not
        //       composing (Path B locale mapping below).
        //   (2) `shift+"` returns CJK single quote instead of double.
        //       `charactersIgnoringModifiers` on some keyboard layouts
        //       returns 0x27 (`'`) for the apostrophe key even when
        //       shift is held. We pass `event` into
        //       `applyLocaleIfApplicable` so it can read the live shift
        //       state and route 0x27+shift to the double-quote map.
        if codepoint < 0x80 && isAsciiPunctKey(codepoint) {
            if session.isComposing {
                if let top = session.commit(at: 0), !top.isEmpty {
                    commitText(top, to: sender)
                }
                candidatePanel?.hide()
                updatePreedit(client: sender)
                // Fall through — punct is now in "not composing" state.
            }
            if let mapped = applyLocaleIfApplicable(
                codepoint: codepoint,
                event: event
            ) {
                commitText(mapped, to: sender)
                return true
            }
            // No CJK mapping (and useCjkPunct may be off) — pass the
            // raw ASCII punct through to host via IMK default routing.
            return false
        }

        // ---- Path C: engine input ------------------------------------------
        let mods = mapModifiers(event.modifierFlags)
        let consumed = session.handleKey(codepoint: codepoint, modifiers: mods)
        if !consumed {
            // Reset smart-quote state on any character the engine didn't take,
            // so the next `"` opens fresh rather than continuing the previous
            // open/close alternation across a sentence boundary.
            session.smartQuoteReset()
            return false
        }

        // Drain pending commit (auto-commit / 5th-letter force / unique match).
        if let committed = session.takeCommit(), !committed.isEmpty {
            commitText(committed, to: sender)
        }
        updatePreedit(client: sender)
        candidatePanel?.refresh(session: session, client: sender as AnyObject?)
        return true
    }

    /// `true` iff `codepoint` is a printable ASCII non-alphanumeric — i.e.,
    /// the characters that *might* belong in CJK punct or smart-quote
    /// territory. Excludes 0–31 (control) and 0x7F.
    private func isAsciiPunctKey(_ codepoint: UInt32) -> Bool {
        guard (0x21...0x7E).contains(codepoint) else { return false }
        let isDigit = (0x30...0x39).contains(codepoint)
        let isUpper = (0x41...0x5A).contains(codepoint)
        let isLower = (0x61...0x7A).contains(codepoint)
        return !isDigit && !isUpper && !isLower
    }

    /// Process a `flagsChanged` event. Routes shift toggles through the
    /// single-click detector; non-shift modifier toggles disarm it. Never
    /// consumes the event (host apps need to see modifier state).
    private func handleFlagsChanged(event: NSEvent, client sender: Any!) -> Bool {
        let kc = event.keyCode
        let isShiftKey =
            (kc == InputxShiftSingleClickDetector.leftShiftKeyCode
                || kc == InputxShiftSingleClickDetector.rightShiftKeyCode)
        if isShiftKey {
            let shiftDown = event.modifierFlags.contains(.shift)
            if shiftDetector.observeShiftFlagsChanged(keyCode: kc, shiftDown: shiftDown) {
                toggleInputMode(client: sender)
            }
        } else {
            shiftDetector.observeOtherModifierChange()
        }
        return false
    }

    /// Flip CJK ↔ EN. Cjk→En with in-flight composing commits the raw
    /// ASCII codes (user signaled "not CJK after all"); En→Cjk has
    /// nothing to drain. Drains `takeCommit()`, refreshes preedit +
    /// candidate UI, broadcasts the new mode for the menubar indicator,
    /// and flashes a brief HUD toast showing "入" / "A".
    private func toggleInputMode(client sender: Any!) {
        let newMode = session.toggleInputMode()
        if let committed = session.takeCommit(), !committed.isEmpty {
            commitText(committed, to: sender)
        }
        updatePreedit(client: sender)
        candidatePanel?.refresh(session: session, client: sender as AnyObject?)
        NotificationCenter.default.post(
            name: .inputxInputModeChanged,
            object: nil,
            userInfo: ["mode": newMode.rawValue]
        )
        InputModeToast.shared.show(mode: newMode)
    }

    // MARK: - System input-source menu integration ---------------------------

    /// Injects entries into the macOS system input-source switcher
    /// dropdown (the menu that drops down when the user clicks the
    /// active input source's title in the menu bar — same menu that
    /// hosts macOS's "编辑自定义短语…" / "显示表情与符号" etc.).
    ///
    /// This is the conventional entry point for IME-specific settings
    /// on macOS — Sogou / Microsoft / Apple's bundled IMEs all hang
    /// their "Preferences…" item here. Our menubar NSStatusItem stays
    /// as a secondary entry, but it's auto-hide-prone + visually
    /// collides with the system "入" indicator, so this menu is the
    /// reliable surface users will discover first.
    override func menu() -> NSMenu! {
        let m = NSMenu(title: "Inputx")
        let settingsItem = NSMenuItem(
            title: "Inputx 设置…",
            action: #selector(openInputxSettings),
            keyEquivalent: ","
        )
        settingsItem.target = self
        settingsItem.keyEquivalentModifierMask = [.command]
        m.addItem(settingsItem)
        m.addItem(.separator())

        let jpItem = NSMenuItem(
            title: "日语扩展（候补に假名 + 共形汉字）",
            action: #selector(toggleJapaneseEnhancement),
            keyEquivalent: ""
        )
        jpItem.target = self
        jpItem.state = inputxSettings.japaneseEnabled ? .on : .off
        m.addItem(jpItem)

        m.addItem(.separator())
        let logItem = NSMenuItem(
            title: "打开 polish 日志（候选未取首位的记录）",
            action: #selector(revealPolishLog),
            keyEquivalent: ""
        )
        logItem.target = self
        m.addItem(logItem)

        return m
    }

    @objc private func revealPolishLog() {
        NSWorkspace.shared.activateFileViewerSelecting([PolishLog.url])
    }

    @objc private func openInputxSettings() {
        SettingsWindowController.shared.show()
    }

    @objc private func toggleJapaneseEnhancement() {
        inputxSettings.japaneseEnabled.toggle()
        NotificationCenter.default.post(
            name: .inputxSettingsChanged,
            object: nil
        )
    }

    // MARK: - Helpers --------------------------------------------------------
    //
    // (The legacy `candidateSelected(...)` override is gone — IMKCandidates'
    // selection callback isn't used by our custom CandidatePanel. Selection +
    // commit is driven directly from `handle()` via Space / arrows / number
    // keys.)

    private func applySettingsToSession() {
        session.setEngineMode(inputxSettings.engineMode)
        session.setAutoCommitPolicy(inputxSettings.autoCommitPolicy)
        session.setJapaneseEnabled(inputxSettings.japaneseEnabled)
    }

    private func mapModifiers(_ flags: NSEvent.ModifierFlags) -> InputxModifiers {
        var m: InputxModifiers = []
        if flags.contains(.shift)    { m.insert(.shift) }
        if flags.contains(.control)  { m.insert(.ctrl) }
        if flags.contains(.option)   { m.insert(.alt) }
        if flags.contains(.command)  { m.insert(.cmd) }
        if flags.contains(.function) { m.insert(.fn) }
        return m
    }

    /// Returns the post-locale-mapping string to insert, or `nil` if no
    /// mapping applied (caller falls through to engine path).
    ///
    /// `event` is read for the shift modifier state, which disambiguates
    /// the quote-key codepoint on layouts where `charactersIgnoringModifiers`
    /// returns 0x27 (apostrophe) regardless of whether shift is held —
    /// pressing shift on the same physical key clearly signals "double
    /// quote intent" and we route accordingly.
    private func applyLocaleIfApplicable(
        codepoint: UInt32,
        event: NSEvent
    ) -> String? {
        guard inputxSettings.useCjkPunct else {
            // Pure full-width mode: only the width toggle applies.
            return inputxSettings.useFullWidth
                ? stringFromCodepoint(InputxLocale.fullWidth(codepoint))
                : nil
        }

        // Quote chars route through the session's stateful smart-quote.
        // Apostrophe + shift → force-interpret as double-quote regardless
        // of what the layout returned. (`charactersIgnoringModifiers` on
        // some layouts returns 0x27 for shift+apostrophe; trust the
        // modifier flag over the layout's mapping.)
        if codepoint == 0x22 /* " */ || codepoint == 0x27 /* ' */ {
            let cp: UInt32 = if codepoint == 0x27
                && event.modifierFlags.contains(.shift) {
                0x22
            } else {
                codepoint
            }
            let mapped = session.smartQuote(cp)
            if mapped != cp {
                return stringFromCodepoint(mapped)
            }
        }

        // Other punct via stateless ASCII→CJK map.
        let asCjk = InputxLocale.asciiToCjk(codepoint)
        if asCjk != codepoint {
            return stringFromCodepoint(asCjk)
        }

        // Full-width applies as a second pass when CJK punct didn't take.
        if inputxSettings.useFullWidth {
            let fw = InputxLocale.fullWidth(codepoint)
            if fw != codepoint {
                return stringFromCodepoint(fw)
            }
        }
        return nil
    }

    private func stringFromCodepoint(_ cp: UInt32) -> String? {
        guard let scalar = Unicode.Scalar(cp) else { return nil }
        return String(scalar)
    }

    private func commitText(_ text: String, to sender: Any?) {
        guard let client = sender as? IMKTextInput else { return }
        client.insertText(
            text,
            replacementRange: NSRange(location: NSNotFound, length: 0)
        )
    }

    private func updatePreedit(client sender: Any?) {
        guard let client = sender as? IMKTextInput else { return }
        if let preedit = session.preedit, !preedit.isEmpty {
            let attr = NSAttributedString(string: preedit)
            client.setMarkedText(
                attr,
                selectionRange: NSRange(location: preedit.count, length: 0),
                replacementRange: NSRange(location: NSNotFound, length: 0)
            )
        } else {
            clearMarkedText(client: sender)
        }
    }

    private func clearMarkedText(client sender: Any?) {
        guard let client = sender as? IMKTextInput else { return }
        client.setMarkedText(
            NSAttributedString(string: ""),
            selectionRange: NSRange(location: 0, length: 0),
            replacementRange: NSRange(location: NSNotFound, length: 0)
        )
    }
}

// `InputxSession.isComposing` already lives in InputxKit's InputxCore.swift —
// no Mac-local extension needed.
