import UIKit
import InputxKit

/// I2 keyboard. Letter / symbol layers, equal-width letter keys, candidate
/// chips at top, haptics, layered punctuation. Layout follows the iOS standard
/// 10-column grid (row1 has 10 letters; row 2 indented half a key; row 3 has
/// special keys taking 1.5 columns each end; row 4 has 1.3 + 1.3 + 5.2 + 2.2,
/// modeled on iOS 26 简体拼音 keyboard: [layer-toggle, emoji, space, return]).
final class KeyboardViewController: UIInputViewController {

    // MARK: - Engine

    private let session = InputxSession()

    // MARK: - Text proxy (item 92 — in-app testing path)

    /// Where `insertText` / `deleteBackward` go. Defaults to the real
    /// `UITextDocumentProxy` (production keyboard-extension path); the
    /// in-app debug keyboard in `InAppKeyboardView` injects a fake that
    /// writes into a SwiftUI binding so maestro can drive the keyboard
    /// without going through iOS's third-party-keyboard activation gate.
    private var _injectedProxy: InputxTextProxy?
    private lazy var _adapterProxy: InputxTextProxy = TextDocumentProxyAdapter(self)
    var inputxProxy: InputxTextProxy {
        return _injectedProxy ?? _adapterProxy
    }
    func setInputxProxy(_ proxy: InputxTextProxy?) {
        _injectedProxy = proxy
    }

    // MARK: - UI

    private var candidateBar: CandidateBar!
    private var keyboardContainer: UIView!
    private var currentLayer: KeyboardLayer = .letters
    private var layerToggleKey: KeyButton?

    // MARK: - Shift state (item 89)
    //
    // Three-state shift matching iOS standard behavior. Letter keys visually
    // flip case via `applyShiftToLetterKeys`; the actual insert path
    // bypasses the IME entirely when shift is engaged — uppercase ASCII is
    // not a valid wubi 字根 letter, and pinyin engines never want capitals.
    private var shiftState: ShiftState = .off {
        didSet {
            guard shiftState != oldValue else { return }
            updateShiftAppearance()
            applyShiftToLetterKeys()
        }
    }
    private var lastShiftTapTime: TimeInterval = 0
    private weak var shiftButton: KeyButton?

    // Geometry derived from the input view's width at layout time.
    private var keyUnit: CGFloat = 32
    private let interKeySpacing: CGFloat = 4
    private let rowHeight: CGFloat = 44
    private let rowSpacing: CGFloat = 6
    private let sideMargin: CGFloat = 4
    private let candidateBarHeight: CGFloat = 40
    private var lastLaidOutWidth: CGFloat = 0

    // MARK: - Lifecycle

    override func viewDidLoad() {
        super.viewDidLoad()
        // Item 68 — semantic background that adapts to dark mode.
        view.backgroundColor = UIColor { trait in
            trait.userInterfaceStyle == .dark
                ? UIColor(white: 0.13, alpha: 1.0)
                : UIColor(white: 0.83, alpha: 1.0)
        }

        // Pull host-settable preferences from the App Group's shared
        // UserDefaults and forward into the Rust core. Re-read on every
        // viewDidLoad so toggling the setting in the main app + reopening
        // the keyboard takes effect without a full process restart.
        let showRare = inputxSharedDefaults.bool(forKey: "showRareChars")
        InputxRareChars.enabled = showRare

        // Phase 4 dual-engine: read engine mode from App Group UserDefaults.
        // Item 51 default seed: missing key returns 0 from .integer(forKey:),
        // which is InputxEngineMode.mixed — exactly the v1 default. So no
        // explicit first-install seed needed; the default takes care of it.
        let modeRaw = UInt8(clamping: inputxSharedDefaults.integer(forKey: "engineMode"))
        let engineMode = InputxEngineMode(rawValue: modeRaw) ?? .mixed
        session.setEngineMode(engineMode)

        // Item 73 — read auto-commit policy from settings (default 3 =
        // OnFourCodesIfUnique). Missing key returns 0 (Never), so seed an
        // explicit default the first time we read it.
        let policyKey = "autoCommitPolicy"
        let hasKey = inputxSharedDefaults.object(forKey: policyKey) != nil
        let policyRaw = UInt32(
            clamping: hasKey
                ? inputxSharedDefaults.integer(forKey: policyKey)
                : 3
        )
        let policy = InputxAutoCommitPolicy(rawValue: policyRaw) ?? .onFourCodesIfUnique
        session.setAutoCommitPolicy(policy)
        if !hasKey {
            // First-install: persist the actual default so the SwiftUI
            // settings picker reads the right value.
            inputxSharedDefaults.set(3, forKey: policyKey)
        }

        // Item 77 — restore L0 from App Group container so user-trained
        // pins / pick counters survive process restarts (kill keyboard,
        // reboot, etc.).
        inputxL0Storage.load(into: session)

        candidateBar = CandidateBar()
        candidateBar.translatesAutoresizingMaskIntoConstraints = false
        candidateBar.onCandidateTap = { [weak self] idx in
            self?.commitCandidate(at: idx)
        }
        // Show/hide the W/P source dot per user preference (item 72 setting,
        // item 78 persistence). Default ON for first install — visual cue
        // helps newcomers learn which engine produced each candidate.
        candidateBar.showSourceIndicator =
            (inputxSharedDefaults.object(forKey: "showSourceIndicator") as? Bool) ?? true
        // Item 88 — pay chip-construction cost at keyboard-load time, not
        // during a keystroke. Pool size matches refreshCandidateCap so a
        // burst from 0→20 candidates never allocates on the hot path.
        candidateBar.preallocateChips(Self.refreshCandidateCap)
        view.addSubview(candidateBar)

        keyboardContainer = UIView()
        keyboardContainer.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(keyboardContainer)

        NSLayoutConstraint.activate([
            candidateBar.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            candidateBar.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            candidateBar.topAnchor.constraint(equalTo: view.topAnchor),
            candidateBar.heightAnchor.constraint(equalToConstant: candidateBarHeight),

            keyboardContainer.topAnchor.constraint(equalTo: candidateBar.bottomAnchor),
            keyboardContainer.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            keyboardContainer.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            keyboardContainer.bottomAnchor.constraint(equalTo: view.bottomAnchor),
        ])

        // Total keyboard view height = candidateBar + 4 rows + spacings + paddings.
        let total = candidateBarHeight + 4 * rowHeight + 3 * rowSpacing + 12
        let h = view.heightAnchor.constraint(equalToConstant: total)
        h.priority = .defaultHigh
        h.isActive = true

        // Cold-start prime — paid on a background thread so the keyboard
        // can show instantly. `InputxSession.warmup` walks every cold path
        // inside inputx-core (wubi dict + FST pages, pinyin dict, 简拼
        // INITIALS_INDEX, wubi→pinyin dispatch) so the user's first
        // keystroke runs on warm caches only. Session FFI is NSLock-
        // serialized, so a keystroke racing with this dispatch will at
        // worst block on the lock for the remainder of the warm — same
        // as the previous sync path, no worse.
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            self?.session.warmup()
        }
    }

    /// One-shot cold-start prime — see `warmUp()` below.
    private var didWarmUp = false

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        let w = view.bounds.width
        guard w > 0, abs(w - lastLaidOutWidth) > 0.5 else { return }
        lastLaidOutWidth = w

        // 10-column reference: width = 10*keyUnit + 9*spacing + 2*margin
        keyUnit = (w - 2 * sideMargin - 9 * interKeySpacing) / 10
        rebuildKeyboard()
        refreshFromSession()
        evaluateAutoCaps()  // initial shift state from empty / cursor context

        if !didWarmUp {
            didWarmUp = true
            // UI warm only — Rust engine already warmed synchronously in
            // viewDidLoad. Deferred one runloop tick so the first frame of
            // the keyboard view renders before we touch the chip pool again.
            DispatchQueue.main.async { [weak self] in
                self?.candidateBar.warmChipLayout()
            }
        }
    }

    override func traitCollectionDidChange(_ previous: UITraitCollection?) {
        super.traitCollectionDidChange(previous)
        // Item 67 — landscape rotation: width changes substantially, force
        // a re-layout. (`viewDidLayoutSubviews` won't fire if the system
        // already laid out at the new width; explicit invalidation here
        // is the safe path.)
        if previous?.verticalSizeClass != traitCollection.verticalSizeClass
            || previous?.horizontalSizeClass != traitCollection.horizontalSizeClass {
            lastLaidOutWidth = 0
            view.setNeedsLayout()
        }
    }

    override func viewWillDisappear(_ animated: Bool) {
        // Item 76 — persist L0 to App Group container. iOS extension
        // viewWillDisappear is the closest analog to applicationWillResignActive
        // for keyboards (extensions don't get the full UIApplication
        // lifecycle); writing here covers normal user flow (close text
        // field, switch app, etc.).
        inputxL0Storage.save(from: session)
        session.clear()
        refreshFromSession()
        super.viewWillDisappear(animated)
    }

    // MARK: - Layer rebuild

    private enum KeyboardLayer { case letters, symbols, emoji }

    private enum ShiftState {
        case off       // lowercase, default
        case temp      // next letter capitalized, then auto-revert to .off
        case locked    // caps lock — stays until manually tapped off
    }

    private func rebuildKeyboard() {
        keyboardContainer.subviews.forEach { $0.removeFromSuperview() }
        layerToggleKey = nil
        shiftButton = nil  // re-assigned when makeShiftCell runs below

        switch currentLayer {
        case .letters: buildRowLayout(rows: letterLayer())
        case .symbols: buildRowLayout(rows: symbolLayer())
        case .emoji:   buildEmojiLayer()
        }

        // Sync visual state to freshly-built keys (letter-case for shift +
        // shift button glyph). Both functions short-circuit when the view
        // tree doesn't contain a shift button or letter keys (emoji layer).
        updateShiftAppearance()
        applyShiftToLetterKeys()
    }

    private func buildRowLayout(rows: [[Cell]]) {
        var prevRow: UIView?
        for cells in rows {
            let row = makeRow(cells: cells)
            keyboardContainer.addSubview(row)
            row.translatesAutoresizingMaskIntoConstraints = false
            NSLayoutConstraint.activate([
                row.centerXAnchor.constraint(equalTo: keyboardContainer.centerXAnchor),
                row.heightAnchor.constraint(equalToConstant: rowHeight),
            ])
            if let prev = prevRow {
                row.topAnchor.constraint(equalTo: prev.bottomAnchor, constant: rowSpacing).isActive = true
            } else {
                row.topAnchor.constraint(equalTo: keyboardContainer.topAnchor, constant: 6).isActive = true
            }
            prevRow = row
        }
    }

    private func buildEmojiLayer() {
        let picker = EmojiPickerView()
        picker.translatesAutoresizingMaskIntoConstraints = false
        picker.onEmojiPicked = { [weak self] emoji in
            self?.inputxProxy.insertText(emoji)
        }
        picker.onBackspace = { [weak self] in
            self?.inputxProxy.deleteBackward()
        }
        picker.onBackToLetters = { [weak self] in
            self?.switchToLayer(.letters)
        }
        keyboardContainer.addSubview(picker)
        NSLayoutConstraint.activate([
            picker.topAnchor.constraint(equalTo: keyboardContainer.topAnchor),
            picker.bottomAnchor.constraint(equalTo: keyboardContainer.bottomAnchor),
            picker.leadingAnchor.constraint(equalTo: keyboardContainer.leadingAnchor),
            picker.trailingAnchor.constraint(equalTo: keyboardContainer.trailingAnchor),
        ])
    }

    /// Switch to a specific layer (used by emoji-key tap + ABC-back-to-letters
    /// from inside the picker). `toggleLayer` still handles the 123 ↔ 拼音
    /// flip on the bottom-left key.
    private func switchToLayer(_ layer: KeyboardLayer) {
        guard currentLayer != layer else { return }
        // Drop any in-flight composing buffer before swapping layers. The
        // user's about to insert content (emoji, symbols) into a different
        // logical context — leaving the engine holding a stale preedit
        // breaks the next deleteBackward count math.
        dropInlinePreedit()
        currentLayer = layer
        rebuildKeyboard()
        refreshFromSession()
        evaluateAutoCaps()
    }

    // MARK: - Row layout

    /// One slot in a row.
    private struct Cell {
        let widthMultiplier: CGFloat
        let view: UIView
    }

    private func makeRow(cells: [Cell]) -> UIView {
        let row = UIStackView()
        row.axis = .horizontal
        row.distribution = .fill
        row.spacing = interKeySpacing
        for cell in cells {
            row.addArrangedSubview(cell.view)
            cell.view.widthAnchor.constraint(
                equalToConstant: cell.widthMultiplier * keyUnit
                    + (cell.widthMultiplier - 1) * interKeySpacing
            ).isActive = true
        }
        return row
    }

    private func makeSpacer(_ widthMultiplier: CGFloat) -> Cell {
        let v = UIView()
        v.translatesAutoresizingMaskIntoConstraints = false
        return Cell(widthMultiplier: widthMultiplier, view: v)
    }

    private func makeLetterCell(_ ch: String) -> Cell {
        let btn = KeyButton(title: ch, style: .letter)
        btn.onTap = { [weak self] in self?.handleLetter(ch) }
        // Item 63 — long-press shows uppercase variant.
        // Inputx's primary use is Chinese (uppercase rare), so a single-
        // variant popup is enough for v1; accent-letter variants land later.
        btn.variants = [ch.uppercased()]
        btn.onVariantPick = { [weak self] picked in
            // Long-press pick bypasses the engine's letter-state machine
            // (uppercase isn't a valid wubi 字根 letter). Insert directly.
            self?.inputxProxy.insertText(picked)
        }
        return Cell(widthMultiplier: 1, view: btn)
    }

    private func makeSymbolCell(_ ch: String) -> Cell {
        let btn = KeyButton(title: ch, style: .letter)
        btn.onTap = { [weak self] in self?.handleSymbol(ch) }
        // Item 64 — long-press shows alternate punctuation.
        if let alts = Self.punctuationAlternates(for: ch) {
            btn.variants = alts
            btn.onVariantPick = { [weak self] picked in
                self?.inputxProxy.insertText(picked)
            }
        }
        return Cell(widthMultiplier: 1, view: btn)
    }

    /// Item 64 — alternate punctuation surfaced via long-press. Mostly
    /// CJK punctuation pairs (the locale layer in Phase 9 will eventually
    /// map ASCII → CJK by context; this is the manual-pick override).
    private static func punctuationAlternates(for ch: String) -> [String]? {
        switch ch {
        case ",":  return ["，", "、", "?"]
        case ".":  return ["。", "…", "·"]
        case "!":  return ["!"]
        case "?":  return ["?", "¿"]
        case ":":  return [":"]
        case ";":  return [";"]
        case "(":  return ["(", "【", "《"]
        case ")":  return [")", "】", "》"]
        // Smart quotes — explicit Unicode escapes to avoid `"""` collisions
        // with Swift's multi-line string syntax. U+2018/U+2019 = single
        // smart quotes; U+201C/U+201D = double smart quotes; U+300C =「.
        case "'":  return ["'", "\u{2018}", "\u{2019}", "\u{201A}"]
        case "\"": return ["\"", "\u{201C}", "\u{201D}", "\u{300C}"]
        case "-":  return ["—", "–", "_"]
        case "/":  return ["÷", "\\"]
        case "$":  return ["¥", "€", "£"]
        case "@":  return ["@"]
        case "#":  return ["♯"]
        case "&":  return ["&"]
        default:   return nil
        }
    }

    private func makeBackspaceCell(width: CGFloat = 1.5) -> Cell {
        let btn = KeyButton(title: "⌫", style: .modifier)
        btn.accessibilityIdentifier = "key-backspace"
        btn.accessibilityLabel = "Backspace"
        btn.onTap = { [weak self] in self?.handleBackspace() }
        return Cell(widthMultiplier: width, view: btn)
    }

    private func makeLayerToggleCell() -> Cell {
        // Inputx is positioned as a 五笔 IME (wubi-primary; pinyin is a fallback
        // path inside the composite engine), so the back-to-letters key reads
        // "五笔" the way iOS Simplified-Pinyin reads "拼音".
        let title = currentLayer == .letters ? "123" : "五笔"
        let btn = KeyButton(title: title, style: .modifier)
        btn.accessibilityIdentifier = "key-layer-toggle"
        btn.onTap = { [weak self] in self?.toggleLayer() }
        layerToggleKey = btn
        return Cell(widthMultiplier: 1.3, view: btn)
    }

    /// Emoji key — switches to the `.emoji` layer (custom in-house picker;
    /// iOS doesn't expose the system emoji picker to third-party keyboards).
    /// Glyph is SF Symbol `face.smiling` so it tints with `.label` and
    /// matches sibling 123/拼音 text keys in light/dark mode.
    private func makeEmojiCell() -> Cell {
        let btn = KeyButton(
            systemImage: "face.smiling",
            accessibilityLabel: "Emoji",
            style: .modifier
        )
        btn.onTap = { [weak self] in self?.switchToLayer(.emoji) }
        return Cell(widthMultiplier: 1.3, view: btn)
    }

    private func makeSpaceCell() -> Cell {
        // Inputx brand mark sits in the bottom-right of the space bar —
        // GOLIA (inputx's parent, golia.jp) — in muted small text. This is
        // the Inputx take on iOS Simplified-Pinyin's centered "拼" mode hint:
        // it doesn't change with mode, it just quietly identifies the IME.
        let btn = KeyButton(title: "GOLIA", style: .letter)
        btn.accessibilityIdentifier = "key-space"
        btn.accessibilityLabel = "Space"
        btn.setTextColor(.tertiaryLabel)
        btn.setFontSize(10, weight: .medium)
        btn.alignLabelToBottomTrailing(insetX: 10, insetY: 4)
        btn.onTap = { [weak self] in self?.handleSpace() }
        return Cell(widthMultiplier: 5.2, view: btn)
    }

    private func makeReturnCell() -> Cell {
        let btn = KeyButton(title: "return", style: .modifier)
        btn.onTap = { [weak self] in self?.handleReturn() }
        return Cell(widthMultiplier: 2.2, view: btn)
    }

    /// `#+=` placeholder for the symbol layer's row3-leftmost slot — iOS's
    /// standard position for the third-tier symbol switcher. Inputx doesn't
    /// have a third tier yet so onTap is a no-op; layout is correct ahead
    /// of any future wiring.
    private func makeSymbolL3Cell() -> Cell {
        let btn = KeyButton(title: "#+=", style: .modifier)
        return Cell(widthMultiplier: 1.5, view: btn)
    }

    private func makeShiftCell() -> Cell {
        let btn = KeyButton(
            systemImage: "shift",
            accessibilityLabel: "Shift",
            style: .modifier
        )
        btn.onTap = { [weak self] in self?.handleShiftTap() }
        shiftButton = btn
        return Cell(widthMultiplier: 1.5, view: btn)
    }

    /// Single-tap toggles temp shift; a second tap within 500ms upgrades to
    /// caps lock. Tap from .locked goes straight to .off. iOS-standard
    /// behavior, implemented via tap-timestamp diff so we don't need a
    /// separate UIGestureRecognizer (which would force `singleTap.require
    /// (toFail: doubleTap)` and add 500ms latency to every single tap).
    /// Threshold 0.5s — slightly more lenient than the 0.3s default to
    /// accommodate XCUITest's variable inter-tap latency during maestro
    /// `doubleTapOn`; real human double-taps comfortably fit under 0.5s.
    private func handleShiftTap() {
        let now = CACurrentMediaTime()
        let isDoubleTap = (now - lastShiftTapTime) < 0.5
        lastShiftTapTime = isDoubleTap ? 0 : now

        if isDoubleTap {
            shiftState = (shiftState == .locked) ? .off : .locked
        } else {
            switch shiftState {
            case .off:    shiftState = .temp
            case .temp:   shiftState = .off
            case .locked: shiftState = .off
            }
        }
    }

    private func updateShiftAppearance() {
        guard let btn = shiftButton else { return }
        switch shiftState {
        case .off:
            btn.setSystemImage("shift")
            btn.setActive(false)
        case .temp:
            btn.setSystemImage("shift.fill")
            btn.setActive(true)
        case .locked:
            btn.setSystemImage("capslock.fill")
            btn.setActive(true)
        }
    }

    /// Mirror `shiftState` onto every letter-style KeyButton's title so the
    /// user sees Q/W/E/... when shift is on, q/w/e/... when off. Letter
    /// `style == .letter` filter skips modifier keys (123/拼音/⌫/return).
    private func applyShiftToLetterKeys() {
        let caps = (shiftState != .off)
        for sub in keyboardContainer.subviews {
            for inner in sub.subviews {
                guard let key = inner as? KeyButton, key.style == .letter else { continue }
                let cur = key.accessibilityLabel ?? ""
                guard let first = cur.first, first.isLetter else { continue }
                let want = caps ? cur.uppercased() : cur.lowercased()
                if cur != want {
                    key.setTitle(want)
                }
            }
        }
    }

    /// AutoCaps stub.
    ///
    /// Inputx is a Chinese IME — Chinese chars have no case, and the original
    /// English-keyboard auto-capitalize-after-sentence-end behavior just
    /// flipped `shiftState` to `.temp` on every fresh document context,
    /// producing wrong-looking uppercase letter keys whenever the user
    /// tapped into a blank field (cf. screenshot of debug input area where
    /// Q W E ... displayed with shift active before any input).
    ///
    /// Kept as a no-op rather than deleted so call sites (handleLetter,
    /// handleBackspace, handleReturn, viewDidLayoutSubviews) don't need to
    /// be edited if/when an explicit English-fallback mode brings AutoCaps
    /// back later (would re-add the body, gated on engine mode).
    private func evaluateAutoCaps() {
        // intentionally empty — see doc comment
    }

    // MARK: - Layouts (10-column grid)

    private func letterLayer() -> [[Cell]] {
        let row1 = ["q","w","e","r","t","y","u","i","o","p"].map(makeLetterCell)
        let row2: [Cell] = [makeSpacer(0.5)]
            + ["a","s","d","f","g","h","j","k","l"].map(makeLetterCell)
            + [makeSpacer(0.5)]
        let row3: [Cell] = [makeShiftCell()]
            + ["z","x","c","v","b","n","m"].map(makeLetterCell)
            + [makeBackspaceCell()]
        let row4: [Cell] = [
            makeLayerToggleCell(),
            makeEmojiCell(),
            makeSpaceCell(),
            makeReturnCell(),
        ]
        return [row1, row2, row3, row4]
    }

    private func symbolLayer() -> [[Cell]] {
        let row1 = ["1","2","3","4","5","6","7","8","9","0"].map(makeSymbolCell)
        let row2: [Cell] = [makeSpacer(0.5)]
            + ["-","/",":",";","(",")","$","&","@"].map(makeSymbolCell)
            + [makeSpacer(0.5)]
        let row3: [Cell] = [makeSymbolL3Cell()]
            + [".",",","?","!","'","\"","#"].map(makeSymbolCell)
            + [makeBackspaceCell()]
        let row4: [Cell] = [
            makeLayerToggleCell(),
            makeEmojiCell(),
            makeSpaceCell(),
            makeReturnCell(),
        ]
        return [row1, row2, row3, row4]
    }

    // MARK: - Engine plumbing

    private func handleLetter(_ ch: String) {
        // Shift engaged → bypass IME entirely, insert uppercase ASCII direct.
        // (Uppercase is not a valid wubi 字根 letter and pinyin doesn't want
        // capitals — explicit shift signals the user wants English text.)
        if shiftState != .off {
            // If we were mid-compose, drop the inline preedit first so the
            // host text view goes from "...zh|" → "...A|" cleanly, not
            // "...zhA|" with leftover composing buffer.
            let oldPreedit = session.preedit ?? ""
            if !oldPreedit.isEmpty {
                eraseHostText(oldPreedit.count)
                session.clear()
            }
            let upper = ch.uppercased()
            inputxProxy.insertText(upper)
            if shiftState == .temp {
                shiftState = .off
            }
            // Refresh the candidate bar — otherwise it shows stale candidates
            // from the just-cleared compose buffer (maestro shift test caught
            // this: host showed "A" but candidate bar still said "wo + 我...").
            refreshFromSession()
            evaluateAutoCaps()
            return
        }

        guard let cp = ch.unicodeScalars.first?.value else { return }
        runEngine(codepoint: cp, fallback: ch)
        evaluateAutoCaps()
    }

    // MARK: - Item 65: AutoCaps for English fallback

    /// Walks the document context behind the cursor; returns true if the
    /// next inserted letter should be uppercased. Heuristic mirrors iOS
    /// system keyboard:
    ///   - Empty document or right after sentence-ender ("." / "!" / "?")
    ///     followed by a space → caps.
    ///   - Right after a newline → caps.
    /// Only applies when NOT actively composing (composing = Chinese
    /// candidates open; uppercase ASCII isn't a wubi letter).
    private func shouldAutoCapitalize() -> Bool {
        guard !session.isComposing else { return false }
        let context = inputxProxy.documentContextBeforeInput ?? ""
        if context.isEmpty { return true }
        let trimmed = context.trimmingCharacters(in: .whitespaces)
        if trimmed.isEmpty { return true }
        if let last = trimmed.last {
            if last == "." || last == "!" || last == "?" {
                // Need at least one space between sentence-ender and cursor.
                return context.hasSuffix(" ")
            }
            if last == "\n" { return true }
        }
        return false
    }

    // (AutoCaps logic moved up: see `evaluateAutoCaps` + `applyShiftToLetterKeys`.
    // ShiftState now drives both the visual letter-key case AND the
    // bypass-IME-on-shift insert path, replacing the old `lastAutoCapsState`
    // boolean cache.)

    private func handleSymbol(_ ch: String) {
        // Route through engine: digits / punctuation may be consumed (digit
        // selects candidate while composing), or pass through cleanly.
        guard let cp = ch.unicodeScalars.first?.value else { return }

        // Phase 9 (item 85) — locale layer. When `useCjkPunct` is on
        // (default true for Chinese IME), map ASCII punct → CJK forms.
        // Smart quotes go through the per-session state (alternates
        // open/close). Full-width letters/digits when `useFullWidth` is on.
        let useCjk = inputxSharedDefaults.object(forKey: "useCjkPunct") as? Bool ?? true
        let useFw = inputxSharedDefaults.object(forKey: "useFullWidth") as? Bool ?? false
        var effective = cp
        var fallback = ch

        if useCjk {
            // Quotes go through smart-quote state machine first.
            // 0x22 = ASCII "  / 0x27 = ASCII '
            if cp == 0x22 || cp == 0x27 {
                let mapped = session.smartQuote(cp)
                if mapped != cp, let scalar = Unicode.Scalar(mapped) {
                    effective = mapped
                    fallback = String(scalar)
                }
            } else {
                let mapped = InputxLocale.asciiToCjk(cp)
                if mapped != cp, let scalar = Unicode.Scalar(mapped) {
                    effective = mapped
                    fallback = String(scalar)
                }
            }
        }
        if useFw {
            // Full-width applies to letters / digits (post-CJK-punct).
            let fw = InputxLocale.fullWidth(effective)
            if fw != effective, let scalar = Unicode.Scalar(fw) {
                effective = fw
                fallback = String(scalar)
            }
        }
        runEngine(codepoint: effective, fallback: fallback)
    }

    private func uintAscii(_ char: Character) -> UInt32 {
        guard let s = char.unicodeScalars.first else { return 0 }
        return s.value
    }

    private func handleSpace() {
        runEngine(codepoint: 0x20, fallback: " ")
    }

    private func handleReturn() {
        // Drop any inline preedit first (we don't commit it on return —
        // keeps behavior predictable for power users), then insert newline.
        let oldPreedit = session.preedit ?? ""
        UIImpactFeedbackGenerator(style: .light).impactOccurred(intensity: 0.6)
        eraseHostText(oldPreedit.count)
        inputxProxy.insertText("\n")
        session.clear()
        refreshFromSession()
        evaluateAutoCaps()
    }

    private func handleBackspace() {
        // 0x7F (DEL forward) is what our Session::handle_key recognizes.
        let oldPreedit = session.preedit ?? ""
        let consumed = session.handleKey(codepoint: 0x7F, modifiers: [])
        if consumed {
            // Engine ate the backspace (e.g. popped one letter off buffer).
            // Sync the host inline preedit: erase old, re-emit committed +
            // new preedit. Same dance as runEngine — backspace can also
            // trigger an auto-commit drain in edge cases.
            eraseHostText(oldPreedit.count)
            if let text = session.takeCommit(), !text.isEmpty {
                inputxProxy.insertText(text)
            }
            let newPreedit = session.preedit ?? ""
            if !newPreedit.isEmpty {
                inputxProxy.insertText(newPreedit)
            }
        } else {
            // Not composing — pass through to the host's text view.
            inputxProxy.deleteBackward()
        }
        refreshFromSession()
        evaluateAutoCaps()
    }

    // MARK: - Inline preedit (item 91)
    //
    // Inputx mirrors the composing buffer into the host text view at the
    // cursor so users can see what they're typing without looking up at
    // the candidate bar's preedit slot. iOS limits this for third-party
    // keyboards — `UITextDocumentProxy` has no `setMarkedText`, so we
    // can't render the buffer with an underline like the system IME does,
    // only insert/delete plain characters. The trick: snapshot the old
    // preedit BEFORE each engine call, then `deleteBackward` that many
    // characters AFTER and `insertText(committed + newPreedit)` in their
    // place. Net effect: buffer appears live at the cursor, commit
    // replaces it with the chosen candidate, no extra characters leaked
    // if the engine forces a partial commit mid-stream.

    /// `deleteBackward` `count` times. preedit chars are ASCII so each
    /// grapheme == one codepoint == one deleteBackward call. Safe over
    /// CJK committed text too (deleteBackward removes one grapheme).
    private func eraseHostText(_ count: Int) {
        guard count > 0 else { return }
        for _ in 0..<count {
            inputxProxy.deleteBackward()
        }
    }

    /// Drop the inline preedit cleanly: erase the host's displayed buffer
    /// AND clear the engine state. Call this before any operation that
    /// would leave the engine and the host text view out of sync — e.g.
    /// layer switches where the user is about to insert non-composing
    /// content (emoji, layer-toggle to symbols, etc.) on top of an
    /// already-visible preedit.
    private func dropInlinePreedit() {
        let preedit = session.preedit ?? ""
        if !preedit.isEmpty {
            eraseHostText(preedit.count)
            session.clear()
        }
    }

    private func runEngine(codepoint: UInt32, fallback: String) {
        #if DEBUG
        let t0 = CFAbsoluteTimeGetCurrent()
        #endif
        let oldPreedit = session.preedit ?? ""
        let consumed = session.handleKey(codepoint: codepoint, modifiers: [])
        #if DEBUG
        let t1 = CFAbsoluteTimeGetCurrent()
        #endif
        // Rebuild the host's inline composing area: erase old preedit,
        // then place [committed text][new preedit] in its slot. Drain
        // takeCommit BEFORE reading the post-state preedit since some
        // engine paths (auto-commit / 5th-letter forced) drop the buffer.
        eraseHostText(oldPreedit.count)
        if let text = session.takeCommit(), !text.isEmpty {
            inputxProxy.insertText(text)
        }
        let newPreedit = session.preedit ?? ""
        if !newPreedit.isEmpty {
            inputxProxy.insertText(newPreedit)
        }
        if !consumed {
            inputxProxy.insertText(fallback)
        }
        #if DEBUG
        let t2 = CFAbsoluteTimeGetCurrent()
        #endif
        refreshFromSession()
        #if DEBUG
        let t3 = CFAbsoluteTimeGetCurrent()
        NSLog("[inputxperf] cp=0x%X consumed=%@ handleKey=%.2fms hostUpdate=%.2fms refresh=%.2fms total=%.2fms",
              codepoint, consumed ? "T" : "F",
              (t1 - t0) * 1000, (t2 - t1) * 1000, (t3 - t2) * 1000, (t3 - t0) * 1000)
        #endif
    }

    private func commitCandidate(at index: Int) {
        let oldPreedit = session.preedit ?? ""
        if let text = session.commit(at: index) {
            // Replace the inline preedit chars with the chosen candidate.
            eraseHostText(oldPreedit.count)
            inputxProxy.insertText(text)
        }
        refreshFromSession()
    }

    private func toggleLayer() {
        // letter ↔ 123 also drops the inline preedit — same reasoning as
        // `switchToLayer`: keeps engine + host text in sync when the user
        // is changing logical input context mid-stream.
        dropInlinePreedit()
        currentLayer = (currentLayer == .letters) ? .symbols : .letters
        rebuildKeyboard()
        refreshFromSession()
    }

    /// Hard cap on candidates fetched per keystroke. Pinyin can return
    /// hundreds; rebuilding that many UIButton chips per refresh makes the
    /// bar feel laggy (system 拼音 keyboard sidesteps this via UICollectionView
    /// cell reuse + visible-window rendering). Cap 20 keeps a buffer past the
    /// ~7 chips fitting first-screen on a 390pt iPhone; users scrolling
    /// further is rare enough to defer lazy-fetch work.
    private static let refreshCandidateCap = 20

    private func refreshFromSession() {
        #if DEBUG
        let t0 = CFAbsoluteTimeGetCurrent()
        #endif
        let preedit = session.preedit ?? ""
        let total = session.candidateCount
        let n = min(total, Self.refreshCandidateCap)
        var cands: [String] = []
        var sources: [InputxCandidateSource?] = []
        cands.reserveCapacity(n)
        sources.reserveCapacity(n)
        for i in 0..<n {
            if let c = session.candidate(at: i) { cands.append(c) }
            // InputxKit returns `.unknown` for indices the engine doesn't
            // attribute; the candidate bar treats `nil` as "no source dot",
            // so collapse `.unknown` → nil here.
            let src = session.candidateSource(at: i)
            sources.append(src == .unknown ? nil : src)
        }
        #if DEBUG
        let t1 = CFAbsoluteTimeGetCurrent()
        #endif
        candidateBar.setState(preedit: preedit, candidates: cands, sources: sources)
        #if DEBUG
        let t2 = CFAbsoluteTimeGetCurrent()
        NSLog("[inputxperf]   refresh totalN=%d shown=%d ffi=%.2fms setState=%.2fms",
              total, n, (t1 - t0) * 1000, (t2 - t1) * 1000)
        #endif
    }
}
