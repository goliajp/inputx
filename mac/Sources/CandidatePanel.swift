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

    /// Whether the panel is currently visually hidden (alpha = 0).
    /// We use alpha toggling instead of orderOut/orderFront because
    /// orderFront has a ~3-9ms compositor / backing-store setup cost
    /// that the user perceives as "switch-to-Inputx 第一次卡". Keeping
    /// the window resident in the window list at alpha 0 collapses the
    /// show cost to a single CALayer property write.
    private var visualHidden: Bool = true
    private var rowViews: [CandidateRow] = []
    /// Last-rendered `(pageIndex, current.count, current[pageStart..<pageEnd])`
    /// fingerprint. Lets `_rebuildRows` early-out when nothing the user
    /// would see changed (a same-page, same-content refresh, e.g.
    /// keystroke that didn't change candidates). Cleared by `hide()` so
    /// re-show always rebuilds.
    private var lastRenderedFingerprint: String = ""
    /// Render-width of the widest visible word at the last layout pass.
    /// Used to skip `layoutSubtreeIfNeeded` + `fittingSize` + `setFrame`
    /// when the new page's widest word still fits — non-shrinking panel
    /// behaviour matches Apple/搜狗/微信 IMEs and keeps the window
    /// from jittering as candidate sets vary. Reset by `hide()`.
    private var cachedMaxWordRenderWidth: CGFloat = 0
    /// Cached attributes for the `NSString.size(withAttributes:)`
    /// measure pass below — building the dict on every refresh would
    /// itself eat a few µs × pageSize.
    private static let wordMeasureAttrs: [NSAttributedString.Key: Any] = [
        .font: NSFont.systemFont(ofSize: 15)
    ]
    /// Per-word render-width cache. Each unique candidate word is
    /// measured exactly once across the IME's lifetime — subsequent
    /// refreshes look up O(1). PerfTimer showed the raw measure pass
    /// at ~1.2ms p50 (10× NSString.size per refresh); the cache
    /// collapses that to a single dict lookup on the common case
    /// where most page words have already been seen.
    private var wordWidthCache: [String: CGFloat] = [:]
    /// One-time AL-calibrated overhead between the widest word's
    /// render width and the panel's content-view width:
    /// `contentWidth - widestWordRenderWidth`. Hand-rolled vs. the
    /// existing constraint stack the overhead comes out to:
    ///   stack edgeInsets.left (8) + row leading-padding (6) +
    ///   numberLabel width (14) + gap (8) + wordLabel trailing-pad (8)
    ///   + stack edgeInsets.right (8) = 52pt.
    /// We measure it empirically on first refresh (one AL pass) to
    /// survive any future constraint tweak without recomputing the
    /// constant by hand. After calibration the layout block becomes
    /// pure arithmetic — no `layoutSubtreeIfNeeded`, no `fittingSize`.
    private var calibratedWidthOverhead: CGFloat?
    /// One-time AL-calibrated panel height. With 10 fixed-height rows
    /// (22pt each), 9× 1pt stack spacing, 6+4 stack edgeInsets, a
    /// stack→footer gap of 2pt, footer intrinsic (~13pt for a
    /// 10pt-font label), and footer bottom-inset of 4pt, the total
    /// settles around 258pt. Always populated together with
    /// `calibratedWidthOverhead`.
    private var calibratedFrameHeight: CGFloat?
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
        // Eager pre-warm everything that would otherwise be paid on
        // the first keystroke's `_rebuildRows`. User-reported "打第
        // 一个字眼皮跳一下" 2026-05-31: cold first refresh took ~20ms
        // (vs warm ~2ms) because three one-time costs all landed on
        // a single keystroke — 10× CandidateRow construction, the AL
        // calibrate pass, and the initial fittingSize measure.
        // Moving them to init() (before the user has typed a thing)
        // makes the first real refresh take the warm path.
        preWarmRows()
    }

    private func preWarmRows() {
        // 1. Build the 10 CandidateRow instances now so the first
        //    refresh's recycling-fast-path can update them in place.
        for _ in 0..<Self.pageSize {
            let row = CandidateRow(numberLabel: "", word: "")
            stack.addArrangedSubview(row)
            rowViews.append(row)
        }

        // 1b. Force CoreText / NSFont glyph cache to populate now by
        //     setting realistic Chinese content on the rows. User
        //     reported "刚安装完的时候明显卡顿" 2026-05-31 — first
        //     text render after a fresh process pays the system-font
        //     glyph-loading cost (~10-30ms), causing the first few
        //     keystrokes to feel laggy. Pre-seeding common characters
        //     warms the font cache.
        let warmupChars = ["我", "你", "他", "的", "是", "在", "了", "中", "国", "人"]
        for (i, row) in rowViews.enumerated() {
            row.update(numberLabel: String(i + 1), word: warmupChars[i])
        }
        // Force render + measure cycle so CoreText actually loads
        // the glyphs and populates the layout caches.
        for w in warmupChars {
            _ = (w as NSString).size(withAttributes: Self.wordMeasureAttrs)
        }
        // 2. Bake the hand-rolled-layout constants. Empirical AL
        //    calibration at first refresh was unreliable (newMaxWidth
        //    depended on whatever the first page words happened to
        //    be → widthOverhead could be over- or under-estimated),
        //    so hardcode the geometry directly from the row + stack
        //    constraints. If the row constraints ever change, both
        //    constants need to be re-derived by hand.
        //
        //    widthOverhead = stack edgeInsets (left 8 + right 8 = 16)
        //                  + row chrome (numberLabel leading 6 + width 14
        //                                + gap 8 + wordLabel trailing 8 = 36)
        //                  = 52pt
        //    frameHeight   = 10× 22pt rows + 9× 1pt stack spacing
        //                  + stack edgeInsets (top 6 + bottom 4 = 10)
        //                  + stack→footer gap 2pt
        //                  + footer intrinsic (~13pt for 10pt-font)
        //                  + footer bottom-inset 4pt
        //                  ≈ 258pt
        calibratedWidthOverhead = 52
        calibratedFrameHeight = 258
        // 3. Force one AL constraint-engine pass to wake the row
        //    layout machinery now. Without it, the first stringValue
        //    update on a row pays the cold constraint-resolution
        //    cost; with it, every refresh runs on warm constraint
        //    state.
        window.contentView?.layoutSubtreeIfNeeded()

        // 4. Pre-warm the window-server's panel setup AND leave the
        //    window resident-but-invisible (alpha 0) so subsequent
        //    show/hide cycles are cheap CALayer alpha writes instead
        //    of the ~3-9ms orderFront compositor setup cost. Set the
        //    frame off-screen first so even a brief alpha glitch
        //    doesn't leak pixels to the user.
        let offScreen = NSRect(x: -10000, y: -10000, width: 110, height: 258)
        window.setFrame(offScreen, display: true)
        window.alphaValue = 0
        window.ignoresMouseEvents = true
        window.orderFront(nil)
        visualHidden = true

        // 5. Clear the warmup content from rows so the first real
        //    refresh sees a known baseline (empty) state.
        for row in rowViews {
            row.update(numberLabel: "", word: "")
        }
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
        PerfTimer.measure("CandidatePanel.refresh") {
            self._refresh(session: session, client: client)
        }
    }

    private func _refresh(session: InputxSession, client: AnyObject?) {
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
        let (count, preeditOpt): (Int, String?) = PerfTimer.measure("session.count+preedit") {
            (session.candidateCount, session.preedit)
        }
        guard count > 0, let preedit = preeditOpt, !preedit.isEmpty else {
            hide()
            return
        }

        let cap = 50
        let words: [String] = PerfTimer.measure("session.candidate(at:)×N") {
            var ws: [String] = []
            let n = min(count, cap)
            ws.reserveCapacity(n)
            for i in 0..<n {
                if let w = session.candidate(at: i) {
                    ws.append(w)
                }
            }
            return ws
        }
        if words != current {
            current = words
            pageIndex = 0
            selectedInPage = 0
        }
        // Reposition when transitioning out of prediction mode — the
        // caret moved while predictions were on (commit advanced it),
        // so the new typing session must anchor at the fresh caret.
        let firstShow = visualHidden
        let needsReposition = firstShow || wasPrediction
        rebuildRows()
        if needsReposition {
            PerfTimer.measure("refresh.positionNear") {
                positionNear(client: client)
            }
        }
        if visualHidden {
            PerfTimer.measure("refresh.show") {
                showVisually()
            }
        }
    }

    private func showVisually() {
        if !visualHidden { return }
        window.alphaValue = 1
        window.ignoresMouseEvents = false
        visualHidden = false
    }

    private func hideVisually() {
        if visualHidden { return }
        window.alphaValue = 0
        window.ignoresMouseEvents = true
        visualHidden = true
    }

    func hide() {
        current.removeAll(keepingCapacity: true)
        isPredictionMode = false
        pageIndex = 0
        selectedInPage = 0
        lastRenderedFingerprint = ""
        cachedMaxWordRenderWidth = 0
        hideVisually()
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
        showVisually()
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
        PerfTimer.measure("CandidatePanel.rebuildRows") {
            self._rebuildRows()
        }
    }

    private func _rebuildRows() {
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
        //
        // Recycling: the 10 row views are created once (first refresh)
        // and reused across every subsequent refresh. Per-call work is
        // `update(numberLabel:word:)` on each row — a couple of
        // `stringValue` setters that no-op on equal strings — plus the
        // highlight + footer updates + AL sizing pass below. Pre-recycle
        // this function was the dominant per-keystroke cost (p50 ~16ms,
        // p95 ~22ms per /tmp/inputx.err.log PerfTimer dumps, 2026-05-31);
        // each refresh tore down 10 rows × (3 subviews + 12 constraints)
        // and rebuilt them. Recycling collapses that to ~10 string
        // compares + a single subtree layout.
        let start = pageIndex * Self.pageSize

        let fingerprint: String = PerfTimer.measure("rR.fingerprint") {
            var fp = "\(pageIndex)|"
            for i in 0..<Self.pageSize {
                let absIdx = start + i
                if absIdx < current.count {
                    fp.append(current[absIdx])
                }
                fp.append("|")
            }
            return fp
        }
        if fingerprint == lastRenderedFingerprint && rowViews.count == Self.pageSize {
            updateRowHighlight()
            updateFooter()
            return
        }
        lastRenderedFingerprint = fingerprint

        // Lazy first-time row creation. Defensive — `preWarmRows` in
        // `init()` already builds the 10 rows, so this branch
        // shouldn't fire post-init. Kept for the safety net case
        // where rowViews got detached somehow.
        if rowViews.count != Self.pageSize {
            for v in rowViews { stack.removeArrangedSubview(v); v.removeFromSuperview() }
            rowViews.removeAll()
            for _ in 0..<Self.pageSize {
                let row = CandidateRow(numberLabel: "", word: "")
                stack.addArrangedSubview(row)
                rowViews.append(row)
            }
        }

        // Two-phase measurement (L2 optimization, 2026-05-31). The
        // typical case is "all page words fit the cached max width"
        // (widthFitHit fires → early-out). For that case we don't
        // need the *exact* new max — we only need to verify that no
        // word exceeds the cached threshold. So:
        //   Phase 1: scan words in order, short-circuit the moment
        //            any one exceeds `cachedMaxWordRenderWidth`.
        //            On full sweep without breach → widthFitHit
        //            fires below; we never compute the exact max.
        //   Phase 2: only when phase 1 found a breach do we measure
        //            ALL words to determine the new max for setFrame.
        //
        // Pre-L2: ~1.12 ms p50 measuring all 10 words upfront on
        // every refresh. Post-L2: phase 1 typically short-circuits
        // on hit or stops at first miss; phase 2 only runs on the
        // ~40% of refreshes where width actually needs to grow.
        var newMaxWidth: CGFloat = 0
        var phase1Breach = false
        PerfTimer.measure("rR.measurePhase1") {
            let threshold = cachedMaxWordRenderWidth
            for i in 0..<Self.pageSize {
                let absIdx = start + i
                guard absIdx < current.count else { continue }
                let word = current[absIdx]
                let ww: CGFloat
                if let cached = wordWidthCache[word] {
                    ww = cached
                } else {
                    ww = (word as NSString)
                        .size(withAttributes: Self.wordMeasureAttrs).width
                    wordWidthCache[word] = ww
                }
                if ww > threshold {
                    phase1Breach = true
                    if ww > newMaxWidth { newMaxWidth = ww }
                    break
                }
                if ww > newMaxWidth { newMaxWidth = ww }
            }
        }
        if phase1Breach {
            PerfTimer.measure("rR.measurePhase2") {
                for i in 0..<Self.pageSize {
                    let absIdx = start + i
                    guard absIdx < current.count else { continue }
                    let word = current[absIdx]
                    let ww = wordWidthCache[word] ?? {
                        let m = (word as NSString)
                            .size(withAttributes: Self.wordMeasureAttrs).width
                        wordWidthCache[word] = m
                        return m
                    }()
                    if ww > newMaxWidth { newMaxWidth = ww }
                }
            }
        }

        PerfTimer.measure("rR.updateRowContent") {
            for i in 0..<Self.pageSize {
                let absIdx = start + i
                let hasWord = absIdx < current.count
                let label = hasWord ? ((i == Self.pageSize - 1) ? "0" : String(i + 1)) : ""
                let word = hasWord ? current[absIdx] : ""
                rowViews[i].update(numberLabel: label, word: word)
            }
        }
        PerfTimer.measure("rR.highlight") { updateRowHighlight() }
        PerfTimer.measure("rR.footer") { updateFooter() }

        if !visualHidden && newMaxWidth <= cachedMaxWordRenderWidth {
            FileHandle.standardError.write(Data("[perf] rR.widthFitHit\n".utf8))
            return
        }
        cachedMaxWordRenderWidth = newMaxWidth

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
        // First-refresh calibration: do exactly one AL fittingSize pass
        // to lock down `widthOverhead` (panel width − widest-word
        // render width) and `frameHeight` (constant for 10 fixed-
        // height rows). All subsequent refreshes compute width from
        // the formula and skip AL entirely — measured ~3ms p50 cost
        // (layoutSubtreeIfNeeded ~1.4ms + fittingSize ~1.6ms) on
        // post-recycling baseline, this drops it to a few µs of arith.
        if calibratedWidthOverhead == nil || calibratedFrameHeight == nil {
            PerfTimer.measure("rR.calibrate") {
                window.contentView?.layoutSubtreeIfNeeded()
                let fitting = window.contentView?.fittingSize
                    ?? NSSize(width: 110, height: 258)
                calibratedWidthOverhead = max(0, fitting.width - newMaxWidth)
                calibratedFrameHeight = fitting.height
            }
        }
        let widthOverhead = calibratedWidthOverhead ?? 52
        let frameHeight = calibratedFrameHeight ?? 258

        // v1.5 width-aware (user 2026-05-24: "字数超过 3 个，候选列表
        // 应该要变宽"). NSTextField .byTruncatingTail was hiding long
        // candidates at fixed 110pt width. Now panel auto-widens to fit
        // the longest candidate, clamped [110, MAX_PANEL_WIDTH] to keep
        // it from spanning the screen.
        PerfTimer.measure("rR.frameBlock") {
            let MIN_WIDTH: CGFloat = 110
            let MAX_WIDTH: CGFloat = 360
            let actualW = max(MIN_WIDTH, min(MAX_WIDTH, newMaxWidth + widthOverhead))
            let actualH = frameHeight
            let currentFrame = window.frame
            var f = currentFrame
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
            // Skip the AppKit call entirely if nothing actually moved.
            // `setFrame(_:display:false)` still walks AppKit's window-
            // server bookkeeping (size class updates, sibling notify,
            // shadow recompute) — measured as the dominant residual
            // cost in `rR.frameBlock` once display: was deferred.
            // The fingerprint early-out already handles the common
            // case (same words → no rebuild), so this guard catches
            // the rarer case of "rebuilt content, same dimensions".
            if currentFrame == f { return }
            PerfTimer.measure("rR.setFrame") {
                // `display: false` — window resizes immediately, the
                // panel's subviews (NSVisualEffectView, 10 rows, footer)
                // redraw lazily on the next runloop display pass.
                // Measured `display: true` cost: 2.68ms p50 of the
                // frameBlock 2.75ms (97% of the cost). For an IME panel
                // that's only growing in width by a few pt to fit a
                // longer candidate, deferring display is visually
                // imperceptible (next runloop turn flushes the redraw
                // queue within one frame), saves ~2.5ms per width-
                // fit-miss refresh. User confirmed "其实还行" 2026-05-31.
                window.setFrame(f, display: false)
            }
        }
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
    /// Cached highlight state. `setHighlighted` short-circuits when
    /// called with the same value — AppKit's `NSTextField.textColor`
    /// + `CALayer.backgroundColor` setters trigger invalidation /
    /// redraw even when the value is identical, so the no-op call
    /// path was costing ~1.8ms per refresh across the 10 rows
    /// (`/tmp/inputx.err.log` PerfTimer dumps showed
    /// `rebuildRows min=1.81ms` even on fingerprint-early-out
    /// paths where only the highlight pass ran).
    private var isHighlightedState: Bool = false

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
        // L1 reverted 2026-05-31: the manual `layout()` override +
        // `intrinsicContentSize=noIntrinsicMetric` approach didn't
        // give NSStackView a clean width-propagation path. Even with
        // an explicit `row.widthAnchor == stack.widthAnchor - 16`
        // pin, the row's bounds.width didn't track the resized
        // window (user reported long-word truncation post-L1). The
        // perf gain (~0.7 ms p50 on rebuildRows) wasn't worth the
        // visual regression. Restoring the 12-constraint internal
        // chain that lets wordLabel.intrinsicContentSize push the
        // row to the right width.
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
        if isHighlightedState == on { return }
        isHighlightedState = on
        bg.layer?.backgroundColor = on
            ? NSColor.selectedContentBackgroundColor.cgColor
            : NSColor.clear.cgColor
        wordLabel.textColor = on ? .selectedMenuItemTextColor : .labelColor
        numberLabel.textColor = on
            ? .selectedMenuItemTextColor.withAlphaComponent(0.8)
            : .secondaryLabelColor
    }

    /// Repoint an already-laid-out row at new content. Avoids the
    /// teardown+reconstruction cost of building a fresh `CandidateRow`
    /// (3 subviews + 12 constraints) on every keystroke. Used by
    /// `_rebuildRows`'s recycling fast path. No-ops on identical
    /// strings to skip the AppKit textStorage invalidate / redraw.
    func update(numberLabel num: String, word: String) {
        if numberLabel.stringValue != num {
            numberLabel.stringValue = num
        }
        if wordLabel.stringValue != word {
            wordLabel.stringValue = word
        }
    }
}
