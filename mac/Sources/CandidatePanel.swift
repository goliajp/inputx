import Cocoa
import InputMethodKit
import InputxKit

/// Custom candidate panel. Replaces the previous `IMKCandidates`-based
/// implementation because Apple's panel doesn't expose enough hooks for the
/// page-numbered UX the user wants:
///   * 1–9 + 0 select positions 1–10 on the current page (no "extras
///     visible but unreachable" model — exactly 10 per page).
///   * ↑ / ↓ move the highlighted selection within the page.
///   * ← / → flip pages (or — Space committed-#0-then-stop semantics
///     handled by IMEController, not here).
///   * Footer shows `x/y 页` so the user knows how much is hiding.
///
/// The window is borderless and a key panel that doesn't steal focus from
/// the host app. It's positioned via the IMK client's caret rect.
final class CandidatePanel {
    /// All candidates from the engine (not just current page).
    private(set) var current: [String] = []
    /// `true` when the panel is showing 联想 (next-word predictions)
    /// instead of regular keystroke-driven candidates. Controls
    /// whether number-key commits route through `session.commit(at:)`
    /// (regular) or `session.commitPrediction(at:)` (prediction).
    /// Set by `showPredictions`, cleared by `hide` / `refresh`.
    private(set) var isPredictionMode: Bool = false
    /// 0-based current page.
    private var pageIndex: Int = 0
    /// 0-based selected index within the current page (0…pageSize-1).
    private var selectedInPage: Int = 0

    /// Per-page candidate count. User-requested 10.
    static let pageSize = 10

    private let window: NSPanel
    private let stack: NSStackView
    private let footer: NSTextField

    private var rowViews: [CandidateRow] = []
    private weak var lastClient: AnyObject?

    /// Which edge of the panel stays put when the row count changes
    /// (page flip, candidate-set refresh):
    ///
    /// * `.top` — panel sits *below* the caret (the common case). The
    ///   panel's TOP edge is anchored where `positionNear` placed it,
    ///   and it grows / shrinks downward.
    /// * `.bottom` — panel sits *above* the caret (when the host input
    ///   field is near the bottom of the screen and there's no room
    ///   below). The panel's BOTTOM edge stays anchored just above the
    ///   caret, and it grows / shrinks upward.
    ///
    /// Without this, flipped-above panels grow into the caret when more
    /// candidates arrive — user-reported bug 2026-05-23.
    private enum AnchorEdge { case top, bottom }
    private var anchorEdge: AnchorEdge = .top

    /// The screen-space Y of the anchored edge — `.top` mode pins the
    /// panel's TOP at this Y, `.bottom` mode pins the panel's BOTTOM
    /// here. Tracked explicitly because `window.frame.origin.y` is NOT
    /// stable between `setFrame` calls — Auto Layout (stack + footer
    /// constraints) reflows after each setFrame and can mutate origin.y
    /// behind our back. Reading `window.frame` next refresh returns a
    /// shifted value, so any anchor-keeping math based on that drifts.
    /// `anchorY` is set by `positionNear` and never touched by AL.
    private var anchorY: CGFloat = 0

    init() {
        // Borderless floating panel — doesn't steal focus, sits above host.
        // Compact width (user-tuned 2x narrower than original 220pt) —
        // numbered rows + word + page indicator only.
        let w = NSPanel(
            contentRect: NSRect(x: 0, y: 0, width: 110, height: 320),
            styleMask: [.nonactivatingPanel, .borderless, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        w.hasShadow = true
        w.isFloatingPanel = true
        w.level = .popUpMenu
        w.collectionBehavior = [.canJoinAllSpaces, .stationary,
                                .fullScreenAuxiliary, .ignoresCycle]
        w.becomesKeyOnlyIfNeeded = true
        w.isMovable = false
        w.hidesOnDeactivate = false
        w.titleVisibility = .hidden
        w.titlebarAppearsTransparent = true
        w.isOpaque = false
        w.backgroundColor = .clear

        // Content view: rounded dark background, vertical stack + footer.
        let content = NSVisualEffectView(
            frame: w.contentView!.bounds
        )
        content.material = .menu
        content.blendingMode = .behindWindow
        content.state = .active
        content.wantsLayer = true
        content.layer?.cornerRadius = 8
        content.layer?.borderWidth = 0.5
        content.layer?.borderColor = NSColor.separatorColor.withAlphaComponent(0.6).cgColor
        content.layer?.masksToBounds = true
        content.autoresizingMask = [.width, .height]
        w.contentView = content

        let stack = NSStackView()
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 1
        stack.edgeInsets = NSEdgeInsets(top: 6, left: 8, bottom: 4, right: 8)
        stack.translatesAutoresizingMaskIntoConstraints = false
        content.addSubview(stack)

        let footer = NSTextField(labelWithString: "")
        footer.font = NSFont.systemFont(ofSize: 10, weight: .regular)
        footer.textColor = .secondaryLabelColor
        footer.alignment = .right
        footer.backgroundColor = .clear
        footer.isBordered = false
        footer.isBezeled = false
        footer.translatesAutoresizingMaskIntoConstraints = false
        content.addSubview(footer)

        NSLayoutConstraint.activate([
            stack.topAnchor.constraint(equalTo: content.topAnchor),
            stack.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: content.trailingAnchor),

            footer.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 8),
            footer.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -8),
            footer.bottomAnchor.constraint(equalTo: content.bottomAnchor, constant: -4),
            footer.topAnchor.constraint(equalTo: stack.bottomAnchor, constant: 2),
        ])

        self.window = w
        self.stack = stack
        self.footer = footer
    }

    /// Update content from the session's current candidate list. Hides
    /// the panel when there's nothing to show.
    ///
    /// Positioning policy: the panel anchors to the caret **only on the
    /// transition from hidden → visible** (i.e., the start of each
    /// preedit session). Subsequent refreshes within the same session
    /// keep the same origin even as the row count grows/shrinks. This
    /// prevents the visual drift the user reported: every refresh
    /// recomputing `positionNear` would creep the panel downward as
    /// IMK's `attributes(forCharacterIndex:)` reported subtly different
    /// caret rects each call. Sticky positioning per session = stable.
    func refresh(session: InputxSession, client: AnyObject?) {
        lastClient = client
        // Refresh always exits prediction mode — predictions only show
        // when there's NO buffer; a normal refresh means buffer changed
        // and we're back to regular keystroke-driven candidates.
        // Capture transition so we can reposition the panel: the post-
        // prediction → new-typing path means the host's caret moved
        // (commit advanced it), and the new composing session should
        // anchor at the FRESH caret, not the stale prediction anchor.
        let wasPrediction = isPredictionMode
        isPredictionMode = false
        let count = session.candidateCount
        guard count > 0, let preedit = session.preedit, !preedit.isEmpty else {
            hide()
            return
        }

        var words: [String] = []
        words.reserveCapacity(count)
        for i in 0..<count {
            if let w = session.candidate(at: i) {
                words.append(w)
            }
        }
        let cap = 50
        if words.count > cap { words.removeLast(words.count - cap) }
        if words != current {
            current = words
            pageIndex = 0
            selectedInPage = 0
        }
        // Reposition when transitioning out of prediction mode — the
        // caret moved while predictions were on (commit advanced it),
        // so the new typing session must anchor at the fresh caret.
        let firstShow = !window.isVisible
        let needsReposition = firstShow || wasPrediction
        rebuildRows()
        if needsReposition {
            positionNear(client: client)
            if !window.isVisible { window.orderFront(nil) }
        }
    }

    func hide() {
        current.removeAll(keepingCapacity: true)
        isPredictionMode = false
        pageIndex = 0
        selectedInPage = 0
        if window.isVisible { window.orderOut(nil) }
    }

    /// Show the panel populated with 联想 (next-word) predictions
    /// instead of buffer-driven candidates. Surfaced after every CJK
    /// commit when `session.predictionCount > 0`. Visual presentation
    /// is identical to the regular panel — same numbering, same anchor
    /// — so the user picks via the same muscle memory (1-9 / 0).
    /// Number-key commit at this point routes through
    /// `session.commitPrediction(at:)` instead of `commit(at:)`, which
    /// triggers a fresh round of predictions (chained 联想 / Sogou
    /// 句串).
    func showPredictions(words: [String], client: AnyObject?) {
        if words.isEmpty {
            hide()
            return
        }
        lastClient = client
        let cap = 50
        var picked = words
        if picked.count > cap { picked.removeLast(picked.count - cap) }
        current = picked
        isPredictionMode = true
        pageIndex = 0
        selectedInPage = 0
        rebuildRows()
        // ALWAYS reposition for predictions — each commit advances the
        // host's caret (the just-committed word shifts everything right),
        // so chained predictions must follow the new caret instead of
        // sticking at the original anchor. This is the
        // post-commit equivalent of "fresh session = fresh position".
        positionNear(client: client)
        if !window.isVisible { window.orderFront(nil) }
    }

    var isVisible: Bool { !current.isEmpty }

    /// Resolve a number-key keystroke (1-9, 0) into the absolute candidate
    /// index on the current page. 1-9 → positions 0-8; 0 → position 9 (i.e.
    /// the 10th candidate). Returns nil if outside the page's filled range.
    func candidateIndex(forNumberKey codepoint: UInt32) -> Int? {
        let inPage: Int?
        if codepoint >= 0x31, codepoint <= 0x39 {
            inPage = Int(codepoint - 0x31)
        } else if codepoint == 0x30 {
            inPage = Self.pageSize - 1   // 0 → 10th slot
        } else {
            inPage = nil
        }
        guard let p = inPage else { return nil }
        let abs = pageIndex * Self.pageSize + p
        return abs < current.count ? abs : nil
    }

    /// Move highlight up (within visible page; doesn't change page).
    /// Returns true if the highlight moved.
    func moveSelectionUp() -> Bool {
        guard isVisible else { return false }
        if selectedInPage > 0 {
            selectedInPage -= 1
            updateRowHighlight()
            return true
        }
        // At top of page — try previous page.
        return prevPage()
    }

    /// Move highlight down.
    func moveSelectionDown() -> Bool {
        guard isVisible else { return false }
        let lastOnPage = min(Self.pageSize, current.count - pageIndex * Self.pageSize) - 1
        if selectedInPage < lastOnPage {
            selectedInPage += 1
            updateRowHighlight()
            return true
        }
        // At bottom of page — try next page.
        return nextPage()
    }

    /// Flip to previous page.
    @discardableResult
    func prevPage() -> Bool {
        guard isVisible, pageIndex > 0 else { return false }
        pageIndex -= 1
        selectedInPage = 0
        rebuildRows()
        return true
    }

    /// Flip to next page.
    @discardableResult
    func nextPage() -> Bool {
        guard isVisible else { return false }
        let totalPages = (current.count + Self.pageSize - 1) / Self.pageSize
        guard pageIndex + 1 < totalPages else { return false }
        pageIndex += 1
        selectedInPage = 0
        rebuildRows()
        return true
    }

    /// Resolve the currently-highlighted candidate to its absolute index.
    func selectedAbsoluteIndex() -> Int? {
        guard isVisible else { return nil }
        let abs = pageIndex * Self.pageSize + selectedInPage
        return abs < current.count ? abs : nil
    }

    // ------------------------------------------------------------- UI build

    private func rebuildRows() {
        for v in rowViews { stack.removeArrangedSubview(v); v.removeFromSuperview() }
        rowViews.removeAll()

        // Always populate ALL 10 slots, even when the current page has
        // fewer candidates (last page, short candidate set). Empty slots
        // get empty label + empty word — they still occupy a standard-
        // height row, so the panel's total height is constant regardless
        // of candidate count.
        //
        // Without this, the stack collapses to (rowCount × rowHeight) and
        // the panel visually shrinks / its unanchored edge slides as
        // candidates flex across pages or keystrokes. The empty label
        // ("" not "5"/"0"/etc.) also keeps the unused 1-9/0 numerals from
        // showing in slots that have no candidate.
        let start = pageIndex * Self.pageSize
        for i in 0..<Self.pageSize {
            let absIdx = start + i
            let hasWord = absIdx < current.count
            let label = hasWord ? ((i == Self.pageSize - 1) ? "0" : String(i + 1)) : ""
            let word = hasWord ? current[absIdx] : ""
            let row = CandidateRow(numberLabel: label, word: word)
            stack.addArrangedSubview(row)
            rowViews.append(row)
        }
        updateRowHighlight()
        updateFooter()

        // Window sizing + positioning. With 10 slots always populated,
        // the *AL-intrinsic* height of the content is constant — but it
        // is NOT what we'd naively compute (22*10 + footer + insets).
        // The actual AL value depends on stack spacings + edgeInsets +
        // footer intrinsic font height + stack→footer gap. Compute it
        // by asking Auto Layout for the contentView's fitting size after
        // a layout pass. Then position the panel from anchorY using that
        // real height.
        //
        // We can't use a hard-coded estimate (`22*10+10+16=246` was
        // wrong by 12pt — AL settles at 258). And `contentMinSize` /
        // `contentMaxSize` don't clamp post-setFrame AL reflow, so
        // setting frame to a too-small height causes AL to inflate AND
        // shift origin.y to keep the panel's TOP edge in place — drifting
        // the anchored BOTTOM down by the inflation delta on every
        // refresh. User-reported "第二个字符输入还是会下偏" 2026-05-23.
        window.contentView?.layoutSubtreeIfNeeded()
        let fitting = window.contentView?.fittingSize
            ?? NSSize(width: 110, height: 22 * CGFloat(Self.pageSize) + 10 + 16)
        // v1.5 width-aware (user 2026-05-24: "字数超过 3 个，候选列表
        // 应该要变宽"). NSTextField .byTruncatingTail was hiding long
        // candidates at fixed 110pt width. Now panel auto-widens to fit
        // the longest candidate, clamped [110, MAX_PANEL_WIDTH] to keep
        // it from spanning the screen.
        let MIN_WIDTH: CGFloat = 110
        let MAX_WIDTH: CGFloat = 360
        let actualW = max(MIN_WIDTH, min(MAX_WIDTH, fitting.width))
        let actualH = fitting.height
        var f = window.frame
        let widthChanged = abs(f.size.width - actualW) > 0.5
        f.size.width = actualW
        f.size.height = actualH
        switch anchorEdge {
        case .top:    f.origin.y = anchorY - actualH
        case .bottom: f.origin.y = anchorY
        }
        // Width changed → re-clamp originX so the panel doesn't fall
        // off the screen right edge (extends leftward when needed).
        if widthChanged, let s = NSScreen.screens.first(where: { $0.frame.contains(NSPoint(x: f.origin.x, y: f.origin.y)) })?.visibleFrame {
            f.origin.x = min(max(s.minX, f.origin.x), s.maxX - f.size.width)
        }
        window.setFrame(f, display: true)
    }

    private func updateRowHighlight() {
        for (i, r) in rowViews.enumerated() {
            r.setHighlighted(i == selectedInPage)
        }
    }

    private func updateFooter() {
        // Page indicator only — the key bindings are obvious from the
        // numbered rows + arrow muscle memory. User explicitly asked to
        // drop the verbose hint so the panel can stretch narrower.
        let totalPages = max(1, (current.count + Self.pageSize - 1) / Self.pageSize)
        footer.stringValue = "\(pageIndex + 1)/\(totalPages) 页"
    }

    // ----------------------------------------------------------- positioning

    /// Place the panel relative to the caret and pick the stable anchor
    /// edge (`anchorEdge`) that `rebuildRows` should hold during
    /// subsequent page flips.
    ///
    /// Default: panel sits below the caret (TOP-anchored). If there's
    /// no room below — host input field is near the bottom of the
    /// screen — flip above the caret and switch to BOTTOM-anchored so
    /// page-flips don't extend the panel downward into the caret.
    private func positionNear(client: AnyObject?) {
        // IMK client typically conforms to NSTextInput / IMKTextInput; both
        // surfaces expose `attributesForCharacterIndex:lineHeightRectangle:`
        // through Obj-C dispatch. We ask for index 0 → returns the rect of
        // the cursor in screen coordinates.
        var caret: NSRect = .zero
        if let c = client as? IMKTextInput {
            var rect: NSRect = .zero
            _ = c.attributes(
                forCharacterIndex: 0,
                lineHeightRectangle: &rect
            )
            caret = rect
        }
        if caret == .zero {
            // Fallback to screen center if client didn't respond.
            if let screen = NSScreen.main {
                caret = NSRect(x: screen.frame.midX, y: screen.frame.midY,
                               width: 0, height: 16)
            }
        }
        var f = window.frame
        let currentH = f.size.height

        // CRITICAL: pick the screen the caret actually sits on — NOT
        // NSScreen.main, which is the *primary* display. On multi-monitor
        // setups when the host app is on a secondary screen, NSScreen.main's
        // visibleFrame.minY is wrong and the flip-above-caret check fires
        // on the wrong screen.
        let caretCenter = NSPoint(x: caret.midX, y: caret.midY)
        let screen = NSScreen.screens.first { $0.frame.contains(caretCenter) }
            ?? NSScreen.main

        // Worst-case panel height: a fully-filled page. The flip decision
        // MUST use this — not `currentH` — because currentH shrinks/grows
        // with the actual candidate count, and basing the flip on the
        // transient height makes `edge` oscillate between `.top` and
        // `.bottom` from one keystroke to the next (verified via NSLog
        // 2026-05-23). With a max-height check, the flip decision is a
        // pure function of caret-vs-screen geometry and stays stable.
        let maxPanelH = 22 * CGFloat(Self.pageSize) + 10 + 16  // 10 rows + footer + padding

        // Shift the panel left by the internal content padding so the
        // number column (the "1" digit) visually aligns with the caret X,
        // not the panel's outer left edge. Internal layout: panel.left
        // → 8pt stack inset → 6pt numberLabel leading → "1" glyph. So
        // 14pt offset lands the number under the caret, matching the
        // system pinyin IME's appearance.
        let contentLeftPadding: CGFloat = 8
        var originX = caret.minX - contentLeftPadding
        let originY: CGFloat
        let edge: AnchorEdge
        let pinnedY: CGFloat  // the screen-Y of the edge we'll hold stable

        if let s = screen?.visibleFrame {
            originX = min(max(s.minX, originX), s.maxX - f.size.width)
            let topModeBottomY = caret.minY - maxPanelH - 4
            if topModeBottomY < s.minY {
                // A max-height panel wouldn't fit below the caret — flip
                // above. Pin the panel's BOTTOM edge just above the caret.
                edge = .bottom
                pinnedY = caret.maxY + 4
                originY = pinnedY
            } else {
                // Room enough for any size below — pin the TOP edge just
                // below the caret. Panel grows downward as candidate
                // count grows; bottom moves, top stays.
                edge = .top
                pinnedY = caret.minY - 4
                originY = pinnedY - currentH
            }
        } else {
            edge = .top
            pinnedY = caret.minY - 4
            originY = pinnedY - currentH
        }
        f.origin = NSPoint(x: originX, y: originY)
        anchorEdge = edge
        anchorY = pinnedY
        window.setFrame(f, display: true)
    }
}

// MARK: - CandidateRow ---------------------------------------------------

/// One row in the candidate panel: small number badge + word. Tracks its
/// own highlighted state for fast updates without re-laying-out the stack.
private final class CandidateRow: NSView {
    private let numberLabel: NSTextField
    private let wordLabel: NSTextField
    private let bg: NSView

    init(numberLabel num: String, word: String) {
        // Background highlight layer.
        let bgView = NSView()
        bgView.wantsLayer = true
        bgView.layer?.cornerRadius = 4
        bgView.layer?.backgroundColor = NSColor.clear.cgColor

        let n = NSTextField(labelWithString: num)
        n.font = NSFont.monospacedDigitSystemFont(ofSize: 11, weight: .medium)
        n.textColor = .secondaryLabelColor
        n.backgroundColor = .clear
        n.isBordered = false

        let w = NSTextField(labelWithString: word)
        w.font = NSFont.systemFont(ofSize: 15)
        w.textColor = .labelColor
        w.backgroundColor = .clear
        w.isBordered = false
        w.lineBreakMode = .byTruncatingTail

        self.bg = bgView
        self.numberLabel = n
        self.wordLabel = w
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        addSubview(bgView)
        addSubview(n)
        addSubview(w)
        bgView.translatesAutoresizingMaskIntoConstraints = false
        n.translatesAutoresizingMaskIntoConstraints = false
        w.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            heightAnchor.constraint(equalToConstant: 22),
            widthAnchor.constraint(greaterThanOrEqualToConstant: 90),

            bgView.topAnchor.constraint(equalTo: topAnchor, constant: 1),
            bgView.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -1),
            bgView.leadingAnchor.constraint(equalTo: leadingAnchor),
            bgView.trailingAnchor.constraint(equalTo: trailingAnchor),

            n.leadingAnchor.constraint(equalTo: bgView.leadingAnchor, constant: 6),
            n.centerYAnchor.constraint(equalTo: bgView.centerYAnchor),
            n.widthAnchor.constraint(equalToConstant: 14),

            w.leadingAnchor.constraint(equalTo: n.trailingAnchor, constant: 8),
            w.centerYAnchor.constraint(equalTo: bgView.centerYAnchor),
            w.trailingAnchor.constraint(equalTo: bgView.trailingAnchor, constant: -8),
        ])
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not used")
    }

    func setHighlighted(_ on: Bool) {
        bg.layer?.backgroundColor = on
            ? NSColor.selectedContentBackgroundColor.cgColor
            : NSColor.clear.cgColor
        wordLabel.textColor = on ? .selectedMenuItemTextColor : .labelColor
        numberLabel.textColor = on
            ? .selectedMenuItemTextColor.withAlphaComponent(0.8)
            : .secondaryLabelColor
    }
}
