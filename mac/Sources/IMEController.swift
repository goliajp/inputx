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
        if let server = server {
            self.candidatePanel = CandidatePanel(server: server)
        }
        // Pay the FST / 简拼-index cold-start cost up front so the first
        // measured keystroke doesn't take ~1-2 s.
        session.warmup()
        // Re-hydrate user-learning state from the on-disk JSON store.
        inputxL0Storage.load(into: session)
        // Process-global rare-CJK toggle reads from prefs at startup.
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

        // Skip Apple PUA (arrow keys, function keys, F1-F19) so they don't
        // get fed to the engine as bogus codepoints. The candidate panel
        // intercepts its own arrow / page keys before this point.
        if (0xF700...0xF8FF).contains(codepoint) {
            return false
        }

        // ---- Path A: number-key candidate commit ---------------------------
        // When the panel is up, 1-9 picks the corresponding candidate
        // without touching the engine state machine.
        if let panel = candidatePanel, panel.isVisible,
           let idx = panel.candidateIndex(forNumberKey: codepoint) {
            if let committed = session.commit(at: idx), !committed.isEmpty {
                commitText(committed, to: sender)
            }
            panel.hide()
            updatePreedit(client: sender)
            return true
        }

        // ---- Path B: symbol / punctuation in zh mode -----------------------
        // The engine doesn't know about CJK punct mapping; we apply it
        // before routing. Only fires when the engine is NOT composing.
        if !session.isComposing && codepoint < 0x80 {
            if let mapped = applyLocaleIfApplicable(codepoint: codepoint) {
                commitText(mapped, to: sender)
                return true
            }
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

    // MARK: - IMKit candidate selection callbacks ----------------------------

    override func candidateSelected(_ candidateString: NSAttributedString!) {
        // User clicked / arrow-key-enter'd a candidate in the panel. We get
        // back the string, not the index — so find it.
        guard let panel = candidatePanel,
              let pickedWord = candidateString?.string,
              let idx = panel.current.firstIndex(of: pickedWord)
        else { return }
        if let committed = session.commit(at: idx), !committed.isEmpty {
            commitText(committed, to: client())
        }
        panel.hide()
        updatePreedit(client: client())
    }

    // MARK: - Helpers --------------------------------------------------------

    private func applySettingsToSession() {
        session.setEngineMode(inputxSettings.engineMode)
        session.setAutoCommitPolicy(inputxSettings.autoCommitPolicy)
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
    private func applyLocaleIfApplicable(codepoint: UInt32) -> String? {
        guard inputxSettings.useCjkPunct else {
            // Pure full-width mode: only the width toggle applies.
            return inputxSettings.useFullWidth
                ? stringFromCodepoint(InputxLocale.fullWidth(codepoint))
                : nil
        }

        // Quote chars route through the session's stateful smart-quote.
        if codepoint == 0x22 /* " */ || codepoint == 0x27 /* ' */ {
            let mapped = session.smartQuote(codepoint)
            if mapped != codepoint {
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
