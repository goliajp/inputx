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

    init() {
        // Borderless floating panel — doesn't steal focus, sits above host.
        // Compact width (≈2/3 of the original 220pt) since the verbose hint
        // text was dropped; just numbered rows + word + page indicator now.
        let w = NSPanel(
            contentRect: NSRect(x: 0, y: 0, width: 150, height: 320),
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
    func refresh(session: InputxSession, client: AnyObject?) {
        lastClient = client
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
        // Cap at 50 — same reasonable upper bound as before. UI shows
        // 10 per page across at most 5 pages.
        let cap = 50
        if words.count > cap { words.removeLast(words.count - cap) }
        if words != current {
            current = words
            // Reset paging on candidate-list change.
            pageIndex = 0
            selectedInPage = 0
        }
        rebuildRows()
        positionNear(client: client)
        if !window.isVisible {
            window.orderFront(nil)
        }
    }

    func hide() {
        current.removeAll(keepingCapacity: true)
        pageIndex = 0
        selectedInPage = 0
        if window.isVisible { window.orderOut(nil) }
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
        // Remove old row views.
        for v in rowViews { stack.removeArrangedSubview(v); v.removeFromSuperview() }
        rowViews.removeAll()

        let start = pageIndex * Self.pageSize
        let end = min(start + Self.pageSize, current.count)
        for (i, idx) in (start..<end).enumerated() {
            let label = (i == Self.pageSize - 1) ? "0" : String(i + 1)
            let row = CandidateRow(numberLabel: label, word: current[idx])
            stack.addArrangedSubview(row)
            rowViews.append(row)
        }
        updateRowHighlight()
        updateFooter()
        // Resize window to fit the new row count.
        let h = max(36, 22 * CGFloat(rowViews.count) + 10 + 16)
        var f = window.frame
        f.size.height = h
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

    /// Anchor the window's top-left just below the client's caret.
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
        // Bottom-left of caret rect → top-left of window (below caret).
        let originX = caret.minX
        let originY = caret.minY - f.size.height - 4
        f.origin = NSPoint(x: originX, y: originY)
        // Clamp to screen.
        if let screen = NSScreen.main {
            let s = screen.visibleFrame
            f.origin.x = min(max(s.minX, f.origin.x), s.maxX - f.size.width)
            if f.origin.y < s.minY {
                f.origin.y = caret.maxY + 4   // flip above caret
            }
        }
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
            widthAnchor.constraint(greaterThanOrEqualToConstant: 130),

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
