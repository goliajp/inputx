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
    // Shared, process-wide panel — IMKit churns controllers per input
    // context; a per-controller NSPanel leaks on dealloc (2026-08-02
    // audit: 1,341 orphaned window clusters / 1.2 GB RSS). See
    // CandidatePanel's class doc.
    private let candidatePanel = CandidatePanel.shared
    /// Detects pure shift single-clicks (no other key in between) to
    /// toggle `InputxInputMode` between `.cjk` and `.en`. See
    /// `InputxShiftSingleClickDetector` for the state machine.
    private let shiftDetector = InputxShiftSingleClickDetector()

    /// macOS virtual keyCode for the CapsLock key (`kVK_CapsLock`). Used
    /// in `handleFlagsChanged` to recognize a CapsLock toggle so we can
    /// drop any in-flight composition.
    private static let capsLockKeyCode: UInt16 = 57

    // MARK: - Segment mode (拼音手动分段) state
    //
    // `segmentAnchorIdx == nil` → auto mode (normal). When the user presses
    // ← while pinyin-composing, we enter segment mode: `segmentAnchors` is a
    // snapshot of the ← stop points (descending prefix lengths with
    // candidates) and `segmentAnchorIdx` indexes into it (0 = largest /
    // longest first segment; ← increments toward shorter, → decrements).
    private var segmentAnchorIdx: Int? = nil
    private var segmentAnchors: [Int] = []
    /// Already-confirmed Chinese prefix during 逐段确认 (e.g. "小明"). Stays in
    /// the marked-text region — caret right after it — until the whole string
    /// is segmented, then the accumulated text is inserted into the host.
    private var segmentCommitted: String = ""

    override init!(server: IMKServer!, delegate: Any!, client inputClient: Any!) {
        super.init(server: server, delegate: delegate, client: inputClient)
        applySettingsToSession()
        // Custom CandidatePanel (no IMKServer needed — see CandidatePanel.swift
        // for why we dropped IMKCandidates in favor of a custom NSWindow).
        // The panel itself is `CandidatePanel.shared`, initialized as a
        // stored-property default above — first controller pays the
        // one-time pre-warm, every later controller reuses the window.
        _ = server
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
        // v1.15 hot-reload observer. Posted by the AppDelegate SIGUSR1
        // handler after `reinstall.py` swaps the pinyin data files
        // under Contents/Resources/data/. Each running InputxController
        // reloads its own PinyinDict from that directory so subsequent
        // keystrokes see freshly-baked polish without a preedit break.
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleDictReloaded),
            name: .inputxDictReloaded,
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

    @objc private func handleDictReloaded() {
        guard let dir = Bundle.main.resourceURL?.appendingPathComponent("data").path else {
            NSLog("Inputx hot-reload: no bundle resource dir")
            return
        }
        let ok = session.reloadEngineData(from: dir)
        NSLog("Inputx hot-reload session=%p dir=%@ ok=%d", self, dir, ok ? 1 : 0)
        // Kick a warmup so the first keystroke after the swap doesn't
        // eat the FST re-walk cost. Warmup is a session method; if it
        // ever fails we still just take the cost on next keystroke.
        session.warmup()
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
        candidatePanel.hide()
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
            candidatePanel.showPredictions(
                words: words,
                client: sender as AnyObject?
            )
        } else {
            candidatePanel.hide()
        }
    }

    // MARK: - Segment mode (拼音手动分段) helpers ----------------------------

    /// Enter segment mode from auto mode. Only when pinyin-composing with ≥1
    /// stop point. Returns false (caller falls back to page-turn) otherwise.
    private func tryEnterSegmentMode(client sender: Any!) -> Bool {
        guard session.isComposing else { return false }
        let anchors = session.segmentAnchors()
        guard !anchors.isEmpty else { return false }
        segmentCommitted = ""
        segmentAnchors = anchors
        segmentAnchorIdx = 0
        refreshSegmentPanel(client: sender)
        return true
    }

    /// Route a key while in segment mode. ← shrink, → grow (idx 0 + → exits),
    /// ↑↓ move selection, digit / space commit the active segment. Letters /
    /// backspace leave the mode (the accumulated Chinese is committed first)
    /// and return false so the caller re-handles the key normally.
    private func handleSegmentKey(codepoint: UInt32, client sender: Any!) -> Bool {
        guard let idx = segmentAnchorIdx, idx < segmentAnchors.count else {
            return false
        }
        switch codepoint {
        case 0xF702: // ← shrink to next shorter anchor (clamp at shortest)
            segmentAnchorIdx = min(idx + 1, segmentAnchors.count - 1)
            refreshSegmentPanel(client: sender)
            return true
        case 0xF703: // → grow; growing past the longest anchor leaves the mode
            if idx == 0 {
                leaveSegmentMode(commitAccumulated: true, client: sender)
                return true
            }
            segmentAnchorIdx = idx - 1
            refreshSegmentPanel(client: sender)
            return true
        case 0xF700: // ↑
            _ = candidatePanel.moveSelectionUp()
            return true
        case 0xF701: // ↓
            _ = candidatePanel.moveSelectionDown()
            return true
        case 0x20: // space → commit highlighted segment candidate
            commitSegmentStep(
                candIdx: candidatePanel.selectedAbsoluteIndex() ?? 0,
                client: sender
            )
            return true
        case 0x1B: // esc → drop everything (accumulated Chinese + buffer)
            session.clear()
            segmentCommitted = ""
            exitSegmentState()
            candidatePanel.hide()
            clearMarkedText(client: sender)
            return true
        default:
            // digit 1-9 / 0 → commit that segment candidate
            if let cand = candidatePanel.candidateIndex(forNumberKey: codepoint) {
                commitSegmentStep(candIdx: cand, client: sender)
                return true
            }
            // letter / backspace / etc. → leave (commit accumulated), then the
            // key gets normal handling by the caller.
            leaveSegmentMode(commitAccumulated: true, client: sender)
            return false
        }
    }

    /// Show the active segment's candidates + render the composition.
    private func refreshSegmentPanel(client sender: Any!) {
        guard let idx = segmentAnchorIdx, idx < segmentAnchors.count else { return }
        let k = segmentAnchors[idx]
        let count = session.segmentCandidateCount(prefixLen: k)
        var words: [String] = []
        words.reserveCapacity(count)
        for i in 0..<count {
            if let w = session.segmentCandidate(prefixLen: k, at: i) {
                words.append(w)
            }
        }
        candidatePanel.showPredictions(words: words, client: sender as AnyObject?)
        updateSegmentPreedit(client: sender)
    }

    /// Commit the active segment as Chinese into `segmentCommitted`; the
    /// remainder re-segments. When nothing's left, insert the accumulated
    /// Chinese into the host and leave segment mode.
    private func commitSegmentStep(candIdx: Int, client sender: Any!) {
        guard let idx = segmentAnchorIdx, idx < segmentAnchors.count else { return }
        let k = segmentAnchors[idx]
        guard let word = session.commitSegment(prefixLen: k, at: candIdx),
              !word.isEmpty else { return }
        segmentCommitted += word
        let remaining = session.preedit ?? ""
        if remaining.isEmpty {
            commitText(segmentCommitted, to: sender)
            segmentCommitted = ""
            exitSegmentState()
            candidatePanel.hide()
        } else {
            segmentAnchors = session.segmentAnchors()
            segmentAnchorIdx = 0
            refreshSegmentPanel(client: sender)
        }
    }

    /// Leave segment mode. If `commitAccumulated`, the already-confirmed
    /// Chinese is inserted into the host first; the remaining pinyin buffer
    /// then falls back to normal auto composition.
    private func leaveSegmentMode(commitAccumulated: Bool, client sender: Any!) {
        if commitAccumulated, !segmentCommitted.isEmpty {
            commitText(segmentCommitted, to: sender)
        }
        segmentCommitted = ""
        exitSegmentState()
        updatePreedit(client: sender)
        if session.isComposing {
            candidatePanel.refresh(session: session, client: sender as AnyObject?)
        } else {
            candidatePanel.hide()
        }
    }

    private func exitSegmentState() {
        segmentAnchorIdx = nil
        segmentAnchors = []
    }

    /// Marked text = `segmentCommitted`(已确认中文) + remaining pinyin. The
    /// active segment `[committed ..< committed+k]` gets a thick underline,
    /// the rest a thin one, and the caret sits right after the committed
    /// Chinese — matching the user's `小明|‸zai‸xizao` spec.
    private func updateSegmentPreedit(client sender: Any?) {
        guard let client = sender as? IMKTextInput,
              let idx = segmentAnchorIdx, idx < segmentAnchors.count else { return }
        let k = segmentAnchors[idx]
        let remaining = session.preedit ?? ""
        let full = segmentCommitted + remaining
        let attr = NSMutableAttributedString(string: full)
        let cN = (segmentCommitted as NSString).length
        let segLen = min(k, (remaining as NSString).length)
        attr.addAttribute(
            .underlineStyle,
            value: NSUnderlineStyle.thick.rawValue,
            range: NSRange(location: cN, length: segLen)
        )
        let restStart = cN + segLen
        let restLen = (full as NSString).length - restStart
        if restLen > 0 {
            attr.addAttribute(
                .underlineStyle,
                value: NSUnderlineStyle.single.rawValue,
                range: NSRange(location: restStart, length: restLen)
            )
        }
        lastPreeditSent = full
        client.setMarkedText(
            attr,
            selectionRange: NSRange(location: cN, length: 0),
            replacementRange: NSRange(location: NSNotFound, length: 0)
        )
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
            if candidatePanel.isVisible, candidatePanel.isPredictionMode {
                candidatePanel.hide()
            }
            // 全角英数 still applies here: EN mode hands raw ASCII to the
            // host, so this is the only place the width toggle can reach
            // letters/digits typed in EN mode. Modifier combos step aside
            // (⌘-shortcuts, ⌥-dead-keys) exactly like the CapsLock path.
            if inputxSettings.useFullWidth,
               !event.modifierFlags.contains(.command),
               !event.modifierFlags.contains(.control),
               !event.modifierFlags.contains(.option),
               let typed = event.characters,
               let scalar = typed.unicodeScalars.first,
               isAsciiAlnum(scalar.value),
               let wide = stringFromCodepoint(InputxLocale.fullWidth(scalar.value)) {
                commitText(wide, to: sender)
                return true
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

        // ---- Segment mode (拼音手动分段, user 2026-06-07) -----------------
        // Once active (user pressed ← while pinyin-composing), ALL keys route
        // through `handleSegmentKey` first and bypass the prediction/normal
        // candidate paths below. `segmentAnchorIdx` is the source of truth;
        // the panel borrows `showPredictions` only to render the segment's
        // candidate list. Keys segment mode doesn't own (letters, etc.) exit
        // the mode and fall through to normal handling.
        if segmentAnchorIdx != nil {
            if handleSegmentKey(codepoint: codepoint, client: sender) {
                return true
            }
            // handleSegmentKey already left segment mode (and committed any
            // accumulated Chinese); fall through to normal handling of this
            // key (letter / backspace).
        }

        // ← enters segment mode from auto mode whenever pinyin-composing —
        // INDEPENDENT of candidate-panel visibility. A long pinyin string like
        // `xiaomingzaixizao` has no whole-string candidate so the panel is
        // hidden; without this, ← would fall through to the host and wipe the
        // marked text. Must run before the PUA arrow / pagination blocks.
        if codepoint == 0xF702, segmentAnchorIdx == nil, session.isComposing,
           tryEnterSegmentMode(client: sender) {
            return true
        }

        // Prediction-mode dismissals. When the panel is showing 联想
        // predictions (post-commit) and the user presses a key that
        // semantically means "I'm done / I don't want a prediction",
        // hide the panel. Letter keys naturally dismiss via Path C's
        // refresh; Esc / Backspace need explicit handling because they
        // wouldn't otherwise reach a panel-refresh call.
        if candidatePanel.isPredictionMode, candidatePanel.isVisible {
            // Esc → dismiss + consume (don't propagate to host).
            if codepoint == 0x1B {
                candidatePanel.hide()
                updatePreedit(client: sender)
                return true
            }
            // Backspace / forward-delete → dismiss + consume (no buffer
            // to delete; the user pressed it to back out of predictions).
            if codepoint == 0x08 || codepoint == 0x7F {
                candidatePanel.hide()
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
                candidatePanel.hide()
                // fall through; Path B below applies locale mapping.
            }
        }

        // Apple PUA range for special keys (arrows, function keys). When
        // the panel is visible, ↑ / ↓ / ← / → drive the panel; other PUA
        // keys still pass through to the host. When the panel is hidden,
        // all PUA passes through.
        if (0xF700...0xF8FF).contains(codepoint) {
            if candidatePanel.isVisible {
                switch codepoint {
                case 0xF700: // up arrow
                    _ = candidatePanel.moveSelectionUp()
                    return true
                case 0xF701: // down arrow
                    _ = candidatePanel.moveSelectionDown()
                    return true
                case 0xF702: // left arrow → previous page (segment-mode entry
                    // is handled earlier, before this block)
                    _ = candidatePanel.prevPage()
                    return true
                case 0xF703: // right arrow → next page
                    _ = candidatePanel.nextPage()
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
        if candidatePanel.isVisible {
            let shifted = event.modifierFlags.contains(.shift)
            switch codepoint {
            case 0x09: // Tab
                if shifted { _ = candidatePanel.prevPage() } else { _ = candidatePanel.nextPage() }
                return true
            case 0x5B, 0x7B: // '[' or '{' — previous page
                _ = candidatePanel.prevPage()
                return true
            case 0x5D, 0x7D: // ']' or '}' — next page
                _ = candidatePanel.nextPage()
                return true
            default:
                break
            }
        }

        // ---- Path A0b: Return → commit highlighted (English fallback) ----
        // User 2026-06-16 (refined): "如果没有上下或 [] 调整过选择的话，
        // 回车是英文上屏". Enter splits on `candidatePanel.selectionTouched`:
        //
        //   - panel visible AND user actively moved selection (↑/↓ or
        //     `[`/`]`) → commit the highlighted candidate (Sogou-style
        //     "commit my pick"). Restores 2026-06-14's selected-commit
        //     semantic for the case where the user expressed intent.
        //
        //   - panel visible BUT untouched → fall through to the
        //     raw-preedit path below. The user typed pinyin and pressed
        //     Enter without picking anything; that's an unambiguous
        //     "I meant English, not Chinese — let me out" intent.
        //
        //   - panel hidden but composing (no candidates) → raw preedit
        //     up-screen so the user isn't trapped by an unmatched
        //     buffer (this branch was already correct).
        //
        //   - nothing in flight → fall through to the host as a literal
        //     newline.
        //
        // Prediction mode follows the same rule: untouched Enter in the
        // 联想 panel = raw preedit (which is empty in prediction mode,
        // so effectively a no-op committed dismissal that lets the
        // user keep typing). Touched Enter commits the selected
        // prediction (chained 联想).
        //
        // 0x0D = main-keyboard Return; 0x03 = numpad Enter (Apple's ETX).
        if codepoint == 0x0D || codepoint == 0x03 {
            if candidatePanel.isVisible, candidatePanel.selectionTouched {
                let idx = candidatePanel.selectedAbsoluteIndex() ?? 0
                if candidatePanel.isPredictionMode {
                    if let committed = session.commitPrediction(at: idx), !committed.isEmpty {
                        commitText(committed, to: sender)
                    }
                } else {
                    let bufferBefore = session.preedit ?? ""
                    let candsBefore = candidatePanel.current
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
                candidatePanel.hide()
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
           candidatePanel.isVisible, candidatePanel.isPredictionMode {
            let idx = candidatePanel.selectedAbsoluteIndex() ?? 0
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
           candidatePanel.isVisible,
           let idx = candidatePanel.selectedAbsoluteIndex(),
           idx > 0
        {
            let bufferBefore = session.preedit ?? ""
            let candsBefore = candidatePanel.current
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

        // ---- Path A-: 全角英数 mode ----------------------------------------
        //
        // `useFullWidth` is a *mode*, not a punct modifier (user 2026-08-08:
        // "打开以后输入直接上屏用日语全角的英文和数字"). While it's on, ASCII
        // letters and digits never reach the engine — they commit straight
        // through as their full-width forms (`nihao` → ｎｉｈａｏ, `123` →
        // １２３), matching macOS 日本語 IM's 「英字（全角）」 mode. Chinese
        // composing resumes the moment the toggle goes back off.
        //
        // Punctuation deliberately stays on Path B: 中文标点 wins there
        // when it's on (`,` → `，`), and the width pass only picks up what
        // the CJK punct table didn't map.
        //
        // Runs ahead of Path A so a digit widens instead of picking a
        // candidate — in this mode the panel can only be a leftover from
        // before the toggle flipped, which the flush below clears.
        if inputxSettings.useFullWidth,
           codepoint < 0x80, isAsciiAlnum(codepoint),
           !event.modifierFlags.contains(.command),
           !event.modifierFlags.contains(.control),
           !event.modifierFlags.contains(.option) {
            // Toggled on mid-composition: land the in-flight候选/preedit
            // first so the full-width text doesn't queue up behind a
            // stranded marked-text area.
            if session.isComposing {
                if let top = session.commit(at: 0), !top.isEmpty {
                    commitText(top, to: sender)
                }
                session.clear()
                candidatePanel.hide()
                clearMarkedText(client: sender)
            } else if session.predictionCount > 0 {
                // Letters normally dismiss 联想 via Path C's refresh; that
                // path is bypassed here, so cancel explicitly.
                session.cancelPredictions()
                candidatePanel.hide()
            }
            if let wide = stringFromCodepoint(InputxLocale.fullWidth(codepoint)) {
                commitText(wide, to: sender)
                return true
            }
            return false
        }

        // ---- Path A: number-key candidate commit ---------------------------
        // When the panel is up, 1-9 + 0 picks the corresponding candidate
        // (0 → 10th slot) without touching the engine state machine.
        if candidatePanel.isVisible,
           let idx = candidatePanel.candidateIndex(forNumberKey: codepoint) {
            // Route based on whether the panel is showing predictions
            // (post-commit 联想) or regular buffer-driven candidates.
            // Predictions commit through `commitPrediction(at:)` which
            // triggers a fresh round of predictions internally (chained
            // 联想 — Sogou 句串 style).
            if candidatePanel.isPredictionMode {
                if let committed = session.commitPrediction(at: idx), !committed.isEmpty {
                    commitText(committed, to: sender)
                }
                showPredictionsOrHide(client: sender)
                updatePreedit(client: sender)
                return true
            }
            let bufferBefore = session.preedit ?? ""
            let candsBefore = candidatePanel.current
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
                } else {
                    // 联想 cancellation — pure-prediction state at the
                    // host side. The engine's handle_key_cjk guard never
                    // sees this codepoint (Path B routes around it via
                    // applyLocaleIfApplicable), so the host must cancel
                    // predictions itself and resync the candidate panel.
                    // Without this, ghost predictions persist after a
                    // user types punct following a CJK commit.
                    if session.predictionCount > 0 {
                        session.cancelPredictions()
                        showPredictionsOrHide(client: sender)
                    }
                }
                if let mapped = applyLocaleIfApplicable(
                    codepoint: codepoint,
                    event: event,
                    client: sender as? IMKTextInput
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
            // Sync the candidate panel with the engine's prediction
            // state. handle_key_cjk's association-cancel guard runs
            // before returning false (digit / Escape / Backspace /
            // arrow / function keys all reach it), so the engine has
            // already dropped its prediction buffer here — but the
            // host's CandidatePanel keeps a local `isPredictionMode`
            // and won't notice unless we tell it. Without this call,
            // the panel keeps showing ghost candidates after the user
            // types a digit following a CJK commit.
            showPredictionsOrHide(client: sender)
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
            candidatePanel.refresh(session: session, client: sender as AnyObject?)
        } else if drained != nil {
            showPredictionsOrHide(client: sender)
        } else {
            candidatePanel.refresh(session: session, client: sender as AnyObject?)
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

    /// `true` iff `codepoint` is an ASCII letter or digit — the set the
    /// 全角英数 mode widens. Punct is excluded: it belongs to Path B, where
    /// 中文标点 gets first refusal before the width pass.
    private func isAsciiAlnum(_ codepoint: UInt32) -> Bool {
        return isAsciiLetter(codepoint) || (0x30...0x39).contains(codepoint)
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
            candidatePanel.hide()
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
        candidatePanel.refresh(session: session, client: sender as AnyObject?)
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
        InputxCellDictRegistry.apply(
            enabledIds: inputxSettings.enabledCellDictPackIds,
            from: InputxCellDictPacksCache.shared.all,
            to: session
        )
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
    /// Longest preceding-text window (UTF-16 units) read for smart-quote
    /// nesting. Quotations are opened/closed within a line in normal
    /// typing; a few hundred units covers realistic lines cheaply. The
    /// Rust side scopes counting to the current line within this window.
    private static let smartQuoteContextWindow = 500

    /// The caret's document context for smart-quote direction.
    private enum CaretContext {
        /// Document text before the caret (possibly empty at doc start).
        case text(String)
        /// Client can't report a caret / surrounding text (terminals,
        /// some web/Electron views) → use the toggle fallback.
        case unavailable
    }

    /// Read the text immediately before the caret from `client`, up to
    /// `smartQuoteContextWindow` UTF-16 units. Returns `.unavailable` when
    /// the client can't report a caret or context.
    private func caretContext(client: IMKTextInput) -> CaretContext {
        let sel = client.selectedRange()
        if sel.location == NSNotFound { return .unavailable }
        if sel.location == 0 { return .text("") }
        let take = min(sel.location, Self.smartQuoteContextWindow)
        let range = NSRange(location: sel.location - take, length: take)
        guard let s = client.attributedSubstring(from: range)?.string else {
            return .unavailable
        }
        return .text(s)
    }

    private func applyLocaleIfApplicable(
        codepoint: UInt32,
        event: NSEvent,
        client: IMKTextInput?
    ) -> String? {
        guard inputxSettings.useCjkPunct else {
            // Pure full-width mode: only the width toggle applies.
            return inputxSettings.useFullWidth
                ? stringFromCodepoint(InputxLocale.fullWidth(codepoint))
                : nil
        }

        // Quote chars: prefer the stateless context path (curly form
        // derived from the caret's preceding character), which survives
        // IME switches, mouse clicks, and mid-text edits. Fall back to the
        // in-memory toggle only when the client can't report context
        // (terminals, some web/Electron views).
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
            let mapped: UInt32 = switch client.map(caretContext(client:)) ?? .unavailable {
            case .text(let ctx):
                session.smartQuoteCtx(cp, contextBefore: ctx)
            case .unavailable:
                session.smartQuote(cp)
            }
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
