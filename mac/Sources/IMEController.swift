import Cocoa
import InputMethodKit
import InputxKit


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

    /// macOS virtual keyCode for the CapsLock key (`kVK_CapsLock`). Used
    /// in `handleFlagsChanged` to recognize a CapsLock toggle so we can
    /// drop any in-flight composition.
    private static let capsLockKeyCode: UInt16 = 57

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

        // Listen for live settings changes (broadcast by SettingsWindow
        // and by this controller's own menu() actions). Without this,
        // the user has to switch input sources out and back to trigger
        // `activateServer` before a freshly toggled JP-enable / engine-
        // mode / policy / locale flag actually reaches the running engine.
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

    /// After any commit, surface the engine's next-word predictions
    /// (联想) in the candidate panel instead of just hiding it. When
    /// `session.predictionCount == 0` this hides the panel as before.
    /// Called from every commit path — manual number-key, space-commit,
    /// auto-commit drain, mode-toggle drain, etc.
    private func showPredictionsOrHide(client sender: Any!) {
        if session.predictionCount > 0 {
            var words: [String] = []
            for i in 0..<session.predictionCount {
                if let w = session.prediction(at: i) {
                    words.append(w)
                }
            }
            candidatePanel?.showPredictions(
                words: words,
                client: sender as AnyObject?
            )
        } else {
            candidatePanel?.hide()
        }
    }

    override func handle(_ event: NSEvent!, client sender: Any!) -> Bool {
        guard let event = event else { return false }

        if event.type == .flagsChanged {
            return handleFlagsChanged(event: event, client: sender)
        }
        guard event.type == .keyDown else { return false }
        // Measure IMK→Swift dispatch latency (kernel + IMK pipeline
        // cost upstream of our handler — the part the user perceives
        // as "switch-to-IME first-keystroke lag" that's invisible to
        // CandidatePanel PerfTimer). NSEvent.timestamp is in the same
        // base as ProcessInfo.processInfo.systemUptime (seconds since
        // boot), so the delta is wall-clock from keyDown to handle()
        // entry.
        let imkLatencyMs = (ProcessInfo.processInfo.systemUptime - event.timestamp) * 1000
        PerfTimer.record(label: "IMK.dispatch", ms: imkLatencyMs)
        // Total Swift handler latency (Path A/B/C dispatch + engine
        // FFI + candidate panel refresh + host text insertion).
        let handlerStart = CFAbsoluteTimeGetCurrent()
        defer {
            let elapsed = (CFAbsoluteTimeGetCurrent() - handlerStart) * 1000
            PerfTimer.record(label: "IMEController.handle", ms: elapsed)
        }
        // Any keyDown disarms the shift detector — shift wasn't alone.
        shiftDetector.observeKeyDown()

        // CapsLock override (user 2026-06-07): while CapsLock is ON the
        // IME enforces uppercase ASCII letters + half-width ASCII punct,
        // regardless of CJK/EN mode, any prior composition, or a held
        // Shift. The in-flight composition (if any) was already dropped
        // when CapsLock toggled on (see `handleFlagsChanged`), so there's
        // no panel/preedit to clean up here.
        //
        //   • Letter keys → commit the UPPERCASE letter directly. We read
        //     `charactersIgnoringModifiers` + `.uppercased()` instead of
        //     passing through, so CapsLock+Shift can't XOR the letter back
        //     to lowercase ("强制大写" — user-confirmed).
        //   • Cmd / Ctrl / Option combos → step aside: ⌘-shortcuts and
        //     ⌥-dead-key / special-character input must reach the host
        //     intact, not be rewritten to a letter.
        //   • Everything else (digits, punct, function / arrow keys) →
        //     step aside; they arrive as raw half-width ASCII.
        if event.modifierFlags.contains(.capsLock) {
            if !event.modifierFlags.contains(.command),
               !event.modifierFlags.contains(.control),
               !event.modifierFlags.contains(.option),
               let base = event.charactersIgnoringModifiers,
               let scalar = base.unicodeScalars.first,
               isAsciiLetter(scalar.value) {
                commitText(base.uppercased(), to: sender)
                return true
            }
            return false
        }

        // EN mode: IME steps aside. Host receives the raw ASCII keystroke
        // (including return / backspace / cmd-combos) directly. We still
        // observed shiftDown above so a subsequent single-shift toggle is
        // detectable; everything else is a pure passthrough.
        //
        // Defensive: if a 联想 prediction panel is up at the moment we
        // enter EN mode, hide it. Predictions are a CJK-mode feature and
        // would otherwise stay visible across the mode boundary while
        // the user types EN letters that this controller doesn't see —
        // producing the user-observed "stale prediction shows next to
        // unrelated typing" bug.
        if session.inputMode == .en {
            if let panel = candidatePanel, panel.isVisible, panel.isPredictionMode {
                panel.hide()
            }
            return false
        }

        guard let chars = event.charactersIgnoringModifiers,
              let firstScalar = chars.unicodeScalars.first
        else { return false }
        var codepoint = firstScalar.value

        // Shift+non-letter re-anchor. `charactersIgnoringModifiers`
        // returns the unshifted ASCII (digit / punct) even when shift
        // is held — but on US/JP/etc keyboards shift+non-letter
        // produces a different glyph that needs to flow through the
        // CJK punct mapping (or Path A's candidate-pick branch for
        // digits).
        //
        // Originally only covered digits (2026-05-24 fix: shift+1 was
        // committing candidate #1 instead of inserting '!'). 2026-05-31
        // user-reported: shift+; produced 全角; instead of 全角:. The
        // root cause is the same — the unshifted char `;` flows into
        // Path B's `:→:`-less semicolon mapping. Extended scope to
        // also cover punct keys so shift+;/'/,/./[/]/-/etc. all reach
        // the locale punct table as their shifted-key form.
        //
        // Letters stay unchanged: shift+a → 'A' is harmless because
        // the engine lowercases internally; routing through this path
        // would set codepoint=0x41 which the engine handles same as
        // 0x61. We exclude option/ctrl/cmd modifier combos to avoid
        // remapping dead-key / shortcut chars.
        if event.modifierFlags.contains(.shift),
           !event.modifierFlags.contains(.option),
           !event.modifierFlags.contains(.control),
           !event.modifierFlags.contains(.command),
           codepoint < 0x80,
           !(0x41...0x5A).contains(codepoint),   // not uppercase letter
           !(0x61...0x7A).contains(codepoint),   // not lowercase letter
           let typed = event.characters,
           let typedScalar = typed.unicodeScalars.first {
            codepoint = typedScalar.value
        }

        // Prediction-mode dismissals. When the panel is showing 联想
        // predictions (post-commit) and the user presses a key that
        // semantically means "I'm done / I don't want a prediction",
        // hide the panel. Letter keys naturally dismiss via Path C's
        // refresh; Esc / Backspace need explicit handling because they
        // wouldn't otherwise reach a panel-refresh call.
        if let panel = candidatePanel, panel.isPredictionMode, panel.isVisible {
            // Esc → dismiss + consume (don't propagate to host).
            if codepoint == 0x1B {
                panel.hide()
                updatePreedit(client: sender)
                return true
            }
            // Backspace / forward-delete → dismiss + consume (no buffer
            // to delete; the user pressed it to back out of predictions).
            if codepoint == 0x08 || codepoint == 0x7F {
                panel.hide()
                updatePreedit(client: sender)
                return true
            }
            // Punctuation / symbol → dismiss panel but DON'T consume —
            // let Path B map the punct (',' → '，' etc.) and commit it
            // normally. Predictions are only meaningful while the user
            // is in a "continuing this sentence" stance; punctuation
            // signals clause/phrase boundary, so the post-commit panel
            // is stale and just clutters the screen.
            // Excludes +/-/= (pagination keys) — those technically
            // satisfy isAsciiPunctKey but are reserved for panel
            // navigation by the block ~50 lines below; hiding here
            // would break their pagination semantics in prediction
            // mode. They naturally never reach Path B for punct
            // mapping because the pagination block consumes them.
            if codepoint < 0x80
                && isAsciiPunctKey(codepoint)
                && codepoint != 0x2B   // '+'
                && codepoint != 0x2D   // '-'
                && codepoint != 0x3D   // '='
            {
                panel.hide()
                // fall through; Path B below applies locale mapping.
            }
        }

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

        // ---- Panel pagination via Tab / [ / ] -----------------------------
        // Same effect as ← / → arrows, exposed under bracket keys. Tab =
        // next page (Shift+Tab = previous), [ = previous, ] = next.
        // Shifted braces { / } follow the unshifted bracket bindings for
        // symmetry (no separate semantics). Intercepts BEFORE Path B
        // (locale punct) so the bracket keystrokes never reach the host
        // when the panel is up.
        //
        // Previously `-` / `+` / `=` were paginate keys; those were freed
        // 2026-05-27 so `-` could be typed as chōonpu (ー) in JP mode
        // (see Path B chōonpu skip below + inputx-nihongo engine `-` accept).
        // User: "`-` 是假名输入中的长音符号，必须要变成可输入的字符".
        if let panel = candidatePanel, panel.isVisible {
            let shifted = event.modifierFlags.contains(.shift)
            switch codepoint {
            case 0x09: // Tab
                if shifted { _ = panel.prevPage() } else { _ = panel.nextPage() }
                return true
            case 0x5B, 0x7B: // '[' or '{' — previous page
                _ = panel.prevPage()
                return true
            case 0x5D, 0x7D: // ']' or '}' — next page
                _ = panel.nextPage()
                return true
            default:
                break
            }
        }

        // ---- Path A0b: Return → commit highlighted (English fallback) ----
        // User 2026-06-14: "回车也应该是选中值上屏，如果候选没有内容就是
        // 英文上屏". Return mirrors Space's selected-commit semantic for
        // the highlight (#0 by default, ↑/↓ moved otherwise), in both
        // regular and prediction modes. When the engine is composing but
        // produced no candidates (panel hidden), Return commits the raw
        // ASCII buffer so the user isn't trapped by an unmatched buffer.
        // When nothing is in flight, Return falls through to the host as
        // a literal newline.
        //
        // 0x0D = main-keyboard Return; 0x03 = numpad Enter (Apple's ETX).
        if codepoint == 0x0D || codepoint == 0x03 {
            if let panel = candidatePanel, panel.isVisible {
                let idx = panel.selectedAbsoluteIndex() ?? 0
                if panel.isPredictionMode {
                    if let committed = session.commitPrediction(at: idx), !committed.isEmpty {
                        commitText(committed, to: sender)
                    }
                } else {
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
                }
                showPredictionsOrHide(client: sender)
                updatePreedit(client: sender)
                return true
            }
            if session.isComposing, let pre = session.preedit, !pre.isEmpty {
                commitText(pre, to: sender)
                session.clear()
                candidatePanel?.hide()
                clearMarkedText(client: sender)
                return true
            }
            return false
        }

        // ---- Path A0a: Space commits the prediction in 联想 mode -------
        // Standard Sogou / 智能ABC behavior: when the candidate panel
        // is showing 联想 predictions, Space commits the highlighted
        // prediction (default: #0). The arrow-driven `idx > 0` carve-
        // out doesn't apply here — in prediction mode every Space is a
        // "commit and continue" gesture. Letter input dismisses
        // predictions (Path C / refresh); Esc / Backspace dismiss
        // explicitly (handled above).
        if codepoint == 0x20,
           let panel = candidatePanel, panel.isVisible, panel.isPredictionMode {
            let idx = panel.selectedAbsoluteIndex() ?? 0
            if let committed = session.commitPrediction(at: idx), !committed.isEmpty {
                commitText(committed, to: sender)
            }
            showPredictionsOrHide(client: sender)
            updatePreedit(client: sender)
            return true
        }

        // ---- Path A0: Space → commit highlighted (not just #0) -----------
        // When the panel is visible and ↑/↓ has moved the highlight off #0,
        // Space commits the *highlighted* candidate. If highlight is on #0
        // (panel just opened), this matches the legacy "Space = commit #0"
        // semantic via Path C below. Falls through if not composing.
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
            showPredictionsOrHide(client: sender)
            updatePreedit(client: sender)
            return true
        }

        // ---- Path A: number-key candidate commit ---------------------------
        // When the panel is up, 1-9 + 0 picks the corresponding candidate
        // (0 → 10th slot) without touching the engine state machine.
        if let panel = candidatePanel, panel.isVisible,
           let idx = panel.candidateIndex(forNumberKey: codepoint) {
            // Route based on whether the panel is showing predictions
            // (post-commit 联想) or regular buffer-driven candidates.
            // Predictions commit through `commitPrediction(at:)` which
            // triggers a fresh round of predictions internally (chained
            // 联想 — Sogou 句串 style).
            if panel.isPredictionMode {
                if let committed = session.commitPrediction(at: idx), !committed.isEmpty {
                    commitText(committed, to: sender)
                }
                showPredictionsOrHide(client: sender)
                updatePreedit(client: sender)
                return true
            }
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
            showPredictionsOrHide(client: sender)
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
            // `-` chōonpu carve-out (user 2026-05-27): when JP is actively
            // composing, route `-` through the engine (Path C below) so it
            // becomes a ー in the kana buffer (`koohi` + `-` → コーヒー).
            // Outside JP composing, fall into the regular Path B punct flow
            // — the host gets a raw / 全角 hyphen.
            if codepoint != 0x2D || !session.isComposingJapanese {
                if session.isComposing {
                    if let top = session.commit(at: 0), !top.isEmpty {
                        commitText(top, to: sender)
                    }
                    showPredictionsOrHide(client: sender)
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
            // else: `-` + JP composing → fall through to Path C, engine
            // accepts the byte as chōonpu input.
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
        let drained = session.takeCommit()
        if let committed = drained, !committed.isEmpty {
            commitText(committed, to: sender)
        }
        updatePreedit(client: sender)
        // If the engine still has a preedit/candidates (user is mid-
        // composing), refresh normally. If the engine just drained a
        // commit and is now idle, show predictions in the panel
        // instead of leaving it empty.
        if session.isComposing {
            candidatePanel?.refresh(session: session, client: sender as AnyObject?)
        } else if drained != nil {
            showPredictionsOrHide(client: sender)
        } else {
            candidatePanel?.refresh(session: session, client: sender as AnyObject?)
        }
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

    /// `true` iff `codepoint` is an ASCII letter (A–Z or a–z). Used by the
    /// CapsLock override to decide which keys to force-uppercase.
    private func isAsciiLetter(_ codepoint: UInt32) -> Bool {
        return (0x41...0x5A).contains(codepoint) || (0x61...0x7A).contains(codepoint)
    }

    /// Process a `flagsChanged` event. Routes shift toggles through the
    /// single-click detector; non-shift modifier toggles disarm it. Never
    /// consumes the event (host apps need to see modifier state).
    private func handleFlagsChanged(event: NSEvent, client sender: Any!) -> Bool {
        let kc = event.keyCode

        // CapsLock toggled (either direction): end any in-flight
        // composition (user 2026-06-07). Rather than discarding the
        // buffer, commit the raw input letters in UPPERCASE — once
        // CapsLock engages, the in-flight pinyin/wubi letters are most
        // likely meant as literal uppercase English, so "上大写" beats
        // "丢弃". `preedit` is the raw lowercased ASCII the user typed
        // (no syllable separators — see pinyin_adapter buffer), so
        // `.uppercased()` yields e.g. "nihao" → "NIHAO". `insertText`
        // replaces + ends the host's marked-text region, same as the
        // Path B punct commit flow.
        //
        // We preserve the CJK/EN input mode: CapsLock is an orthogonal
        // uppercase-ASCII override, not a mode switch. `session.clear()`
        // resets the core to default CJK mode as a side effect, so we
        // re-apply the saved mode afterward (a no-op when it was already
        // CJK; a clean state-flip for EN since clear() left nothing).
        if kc == Self.capsLockKeyCode {
            let savedMode = session.inputMode
            if session.isComposing, let pre = session.preedit, !pre.isEmpty {
                commitText(pre.uppercased(), to: sender)
            }
            session.clear()
            if savedMode != .cjk { _ = session.setInputMode(savedMode) }
            candidatePanel?.hide()
            clearMarkedText(client: sender)
            // CapsLock isn't shift; disarm any half-armed shift single-click.
            shiftDetector.observeOtherModifierChange()
            return false
        }

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
        InputModeToast.shared.show(mode: newMode)
    }

    // MARK: - System input-source menu integration ---------------------------

    /// Two-tier settings architecture:
    ///
    /// - **IMK menu (this method)** — slim, only the toggles a user
    ///   flips frequently while typing: engine mode (per-task language
    ///   switch), JP attachment (situational), CJK punctuation /
    ///   full-width digits (per writing context). Plus the "Inputx
    ///   设置…" entry into the full panel.
    /// - **Settings window (`SettingsWindowController`)** — everything
    ///   else: auto-commit policy (set-once config), 显示生僻字 toggle
    ///   (set-once after font install), L0 learning sub-actions
    ///   (打开数据目录 / polish 日志 / 重置 — diagnostic, infrequent),
    ///   about / version info.
    ///
    /// Guiding principle per user feedback [[feedback-imk-menu-minimal]]:
    /// IMK menu is a high-frequency glance surface, not a config panel.
    /// New items default to Settings window unless there's evidence the
    /// user toggles them multiple times a session.
    ///
    /// Rebuilt fresh every time macOS asks for it, so radio/toggle
    /// states reflect live settings without needing manual refresh.
    override func menu() -> NSMenu! {
        let m = NSMenu(title: "Inputx")

        // Settings window — first item for discoverability + ⌘, keystroke.
        let openSettings = NSMenuItem(
            title: "Inputx 设置…",
            action: #selector(openInputxSettings),
            keyEquivalent: ","
        )
        openSettings.target = self
        openSettings.keyEquivalentModifierMask = [.command]
        m.addItem(openSettings)
        m.addItem(.separator())

        // Engine mode picker (header + 4 radio items).
        let modeHeader = NSMenuItem(title: "输入方案", action: nil, keyEquivalent: "")
        modeHeader.isEnabled = false
        m.addItem(modeHeader)
        addModeItem(m, "混合（五笔为主，拼音兜底）", mode: .mixed)
        addModeItem(m, "仅五笔", mode: .wubiOnly)
        addModeItem(m, "仅拼音", mode: .pinyinOnly)
        addModeItem(m, "仅日语", mode: .japaneseOnly)
        m.addItem(.separator())

        // Japanese plugin attachment — only meaningful under Chinese
        // engine modes; under .japaneseOnly the toggle is implicit.
        if inputxSettings.engineMode != .japaneseOnly {
            let jp = NSMenuItem(
                title: "日本語拡張（候補に平仮名・片仮名・漢字を追加）",
                action: #selector(toggleJapaneseEnhancement),
                keyEquivalent: ""
            )
            jp.target = self
            jp.state = inputxSettings.japaneseEnabled ? .on : .off
            m.addItem(jp)
            m.addItem(.separator())
        }

        // Locale toggles — per writing context (Chinese prose vs code
        // / mixed-English text), so high-frequency enough to stay in
        // the menu.
        addToggle(m, "中文标点（，。？！…）",
                  isOn: inputxSettings.useCjkPunct,
                  selector: #selector(toggleCjkPunct))
        addToggle(m, "英文数字全角",
                  isOn: inputxSettings.useFullWidth,
                  selector: #selector(toggleFullWidth))

        // NOT shown here (in the Settings window instead):
        //   - 自动上屏 policy radio    — set-once config
        //   - 显示生僻字 toggle         — set-once after font install
        //   - 学习记录 (L0) sub-items   — diagnostic, infrequent
        //   - 关于 Inputx              — informational, one-off
        return m
    }

    // MARK: - menu() item builders -------------------------------------------

    private func addModeItem(_ m: NSMenu, _ title: String, mode: InputxEngineMode) {
        let item = NSMenuItem(title: title,
                              action: #selector(pickMode(_:)),
                              keyEquivalent: "")
        item.target = self
        item.tag = Int(mode.rawValue)
        item.state = (inputxSettings.engineMode == mode) ? .on : .off
        m.addItem(item)
    }

    private func addToggle(_ m: NSMenu, _ title: String, isOn: Bool, selector: Selector) {
        let item = NSMenuItem(title: title, action: selector, keyEquivalent: "")
        item.target = self
        item.state = isOn ? .on : .off
        m.addItem(item)
    }

    // MARK: - menu() actions -------------------------------------------------

    @objc private func openInputxSettings() {
        SettingsWindowController.shared.show()
    }

    @objc private func pickMode(_ sender: NSMenuItem) {
        guard let mode = InputxEngineMode(rawValue: UInt8(sender.tag)) else { return }
        inputxSettings.engineMode = mode
        broadcastSettingsChanged()
    }

    @objc private func toggleCjkPunct() {
        inputxSettings.useCjkPunct.toggle()
        broadcastSettingsChanged()
    }

    @objc private func toggleFullWidth() {
        inputxSettings.useFullWidth.toggle()
        broadcastSettingsChanged()
    }

    @objc private func toggleJapaneseEnhancement() {
        inputxSettings.japaneseEnabled.toggle()
        broadcastSettingsChanged()
    }

    private func broadcastSettingsChanged() {
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
        // Host accepted the text + cleared its marked-text area; sync our
        // cache so the next `updatePreedit("")` correctly recognizes the
        // host as already-cleared and short-circuits.
        lastPreeditSent = nil
        PerfTimer.measure("IMK.insertText") {
            client.insertText(
                text,
                replacementRange: NSRange(location: NSNotFound, length: 0)
            )
        }
    }

    /// Last preedit string actually delivered to the host via
    /// `setMarkedText`. `updatePreedit` consults this cache to skip
    /// the IMK IPC when the new preedit matches — each `setMarkedText`
    /// is a cross-process round-trip (PerfTimer measured ~1ms p50),
    /// and the 9 different `updatePreedit` call sites in `handle`
    /// occasionally fire back-to-back with the same content (commit
    /// drain → predictions setup → refresh, all touching the same
    /// empty/active preedit). `nil` means "host's marked-text area is
    /// known empty" (right after launch, post-commit, post-deactivate).
    private var lastPreeditSent: String? = nil

    private func updatePreedit(client sender: Any?) {
        guard let client = sender as? IMKTextInput else { return }
        let preedit = session.preedit ?? ""
        if preedit.isEmpty {
            // No marked text. Skip the IPC if the host is already cleared.
            if lastPreeditSent == nil { return }
            lastPreeditSent = nil
            PerfTimer.measure("IMK.setMarkedText(clear)") {
                client.setMarkedText(
                    NSAttributedString(string: ""),
                    selectionRange: NSRange(location: 0, length: 0),
                    replacementRange: NSRange(location: NSNotFound, length: 0)
                )
            }
            return
        }
        // Skip the IPC if the host already has this exact preedit string.
        if lastPreeditSent == preedit { return }
        lastPreeditSent = preedit
        let attr = NSAttributedString(string: preedit)
        PerfTimer.measure("IMK.setMarkedText(update)") {
            client.setMarkedText(
                attr,
                selectionRange: NSRange(location: preedit.count, length: 0),
                replacementRange: NSRange(location: NSNotFound, length: 0)
            )
        }
    }

    private func clearMarkedText(client sender: Any?) {
        guard let client = sender as? IMKTextInput else { return }
        if lastPreeditSent == nil { return }
        lastPreeditSent = nil
        PerfTimer.measure("IMK.setMarkedText(clear)") {
            client.setMarkedText(
                NSAttributedString(string: ""),
                selectionRange: NSRange(location: 0, length: 0),
                replacementRange: NSRange(location: NSNotFound, length: 0)
            )
        }
    }
}

// `InputxSession.isComposing` already lives in InputxKit's InputxCore.swift —
// no Mac-local extension needed.
