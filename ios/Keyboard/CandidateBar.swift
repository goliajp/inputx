import UIKit
import CoreText
import InputxKit

/// Reusable chip — replaces per-keystroke UIButton construction. CandidateBar
/// keeps a lazy-grown pool of these; setState only updates label text + source
/// dot visibility, sidestepping allocator / Auto Layout churn that previously
/// dominated the candidate refresh hot path. Single-tap commits via `onTap`;
/// long-press surfaces Unihan info via `onLongPress`.
final class ChipView: UIView {
    var index: Int = 0
    var onTap: ((Int) -> Void)?
    var onLongPress: ((Int, UIView) -> Void)?

    private let label = UILabel()
    private let bg = UIView()
    private let dot = UIView()

    var font: UIFont? {
        didSet { label.font = font }
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        setup()
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) not used") }

    private func setup() {
        translatesAutoresizingMaskIntoConstraints = false

        bg.backgroundColor = .clear
        bg.layer.cornerRadius = 6
        bg.layer.cornerCurve = .continuous
        bg.isUserInteractionEnabled = false
        bg.translatesAutoresizingMaskIntoConstraints = false
        addSubview(bg)

        label.textColor = .label
        label.textAlignment = .center
        label.isUserInteractionEnabled = false
        label.translatesAutoresizingMaskIntoConstraints = false
        addSubview(label)

        dot.translatesAutoresizingMaskIntoConstraints = false
        dot.layer.cornerRadius = 2.0
        dot.isUserInteractionEnabled = false
        dot.isAccessibilityElement = false
        dot.isHidden = true
        addSubview(dot)

        NSLayoutConstraint.activate([
            bg.topAnchor.constraint(equalTo: topAnchor, constant: 4),
            bg.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -4),
            bg.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 2),
            bg.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -2),

            label.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 12),
            label.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -12),
            label.topAnchor.constraint(equalTo: topAnchor, constant: 4),
            label.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -4),

            dot.widthAnchor.constraint(equalToConstant: 4),
            dot.heightAnchor.constraint(equalToConstant: 4),
            dot.topAnchor.constraint(equalTo: topAnchor, constant: 3),
            dot.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -3),
        ])

        let tap = UITapGestureRecognizer(target: self, action: #selector(handleTap))
        addGestureRecognizer(tap)

        let lp = UILongPressGestureRecognizer(target: self, action: #selector(handleLongPress(_:)))
        lp.minimumPressDuration = 0.45
        addGestureRecognizer(lp)

        isAccessibilityElement = true
        accessibilityTraits = .button
        accessibilityHint = "长按查看字符信息"
    }

    func update(
        text: String,
        source: InputxCandidateSource?,
        index: Int,
        showSourceDot: Bool,
        accessibilityLabel: String
    ) {
        // Skip-if-equal — UILabel.text setter would otherwise invalidate
        // intrinsicContentSize and bubble a layout pass up to UIStackView
        // even when the chip's value didn't change.
        if label.text != text { label.text = text }
        self.index = index
        if self.accessibilityLabel != accessibilityLabel {
            self.accessibilityLabel = accessibilityLabel
        }
        // Stable identifier for maestro / XCUITest. `candidate-你` matches
        // exactly one chip even if the "你" character also appears in
        // surrounding host text or other UI labels.
        accessibilityIdentifier = "candidate-\(text)"
        if showSourceDot, let src = source {
            let color: UIColor = (src == .wubi) ? .systemBlue : .systemOrange
            if dot.backgroundColor != color { dot.backgroundColor = color }
            if dot.isHidden { dot.isHidden = false }
        } else {
            if !dot.isHidden { dot.isHidden = true }
        }
    }

    @objc private func handleTap() {
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        bg.backgroundColor = UIColor.systemFill
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) { [weak self] in
            self?.bg.backgroundColor = .clear
        }
        onTap?(index)
    }

    @objc private func handleLongPress(_ gr: UILongPressGestureRecognizer) {
        guard gr.state == .began else { return }
        UIImpactFeedbackGenerator(style: .medium).impactOccurred()
        onLongPress?(index, self)
    }
}

/// Top-of-keyboard preedit + scrollable, tappable candidate row.
///
/// Phase 6 (item 55-62) candidate bar polish:
/// - Free-scroll horizontal layout (matches Apple / Sogou / Baidu iOS IME
///   conventions; explicit paging would feel non-native for variable-width
///   chips, so item 55's "paging" is interpreted as "smooth gesture-driven
///   scroll" — already provided by UIScrollView with `decelerationRate =
///   .fast`).
/// - Item 56: position indicator label "N / total" at the trailing edge.
/// - Item 57: 4pt W/P source dot at top-right of each chip.
/// - Item 58: touch-down background highlight via ChipView's own bg toggle.
/// - Item 59: long-press → Unihan info popup (codepoint + char metadata).
/// - Item 61: VoiceOver labels per chip + scrollview accessibility hint.
/// - Item 62: Dynamic Type — `UIFontMetrics` scales the chip font with the
///   user's content-size pref, clamped to [14, 22] pt to keep layout sane
///   inside the IME's height budget.
/// - Item 88 (perf): ChipView pool reuse + cached font. Per-keystroke refresh
///   no longer allocates/destroys chips; only label.text + source-dot
///   visibility flip. Pool grows lazily, never shrinks (amortizes setup).
final class CandidateBar: UIView {
    // MARK: - Font + Dynamic Type (item 62)

    /// Base point size before Dynamic Type scaling.
    private static let basePointSize: CGFloat = 18
    /// Hard clamp on the user-scaled candidate font: [14, 22] pt.
    /// IME bar is height-budgeted; uncapped Dynamic Type would clip.
    private static let minPointSize: CGFloat = 14
    private static let maxPointSize: CGFloat = 22

    /// Build the cascade-list candidate font (PingFang + Lab8CJKExtended
    /// fallback for CJK Ext B+). Cached on the instance and only rebuilt
    /// when Dynamic Type actually changes — not per keystroke.
    private static func candidateFont(for traits: UITraitCollection) -> UIFont {
        let metrics = UIFontMetrics(forTextStyle: .body)
        let scaled = metrics.scaledValue(
            for: basePointSize,
            compatibleWith: traits
        )
        let clamped = max(minPointSize, min(maxPointSize, scaled))
        let base = UIFont.systemFont(ofSize: clamped, weight: .regular)
        guard let extFont = UIFont(name: "Lab8CJKExtended-Regular", size: clamped)
            ?? UIFont(name: "Lab8 CJK Extended", size: clamped)
        else {
            return base
        }
        let cascade = [extFont.fontDescriptor]
        let descriptor = base.fontDescriptor.addingAttributes([
            .cascadeList: cascade
        ])
        return UIFont(descriptor: descriptor, size: clamped)
    }

    var onCandidateTap: ((Int) -> Void)?
    /// Long-press handler — receives the candidate index. Default
    /// implementation shows a Unihan-info popup attached to the chip.
    var onCandidateLongPress: ((Int, UIView) -> Void)?

    private let preeditLabel = UILabel()
    private let separator = UIView()
    private let scroll = UIScrollView()
    private let stack = UIStackView()
    /// Item 56 — "N / total" counter at the trailing edge. Hidden when the
    /// candidate list fits without scrolling; shown when overflow exists.
    private let countLabel = UILabel()

    /// Cached candidate strings — used by the long-press popup so it can
    /// look up text by index without re-querying the session.
    private var lastCandidates: [String] = []
    private var lastSources: [InputxCandidateSource?] = []

    /// Reusable chip pool. Lazily grown; never shrunk. Index in this array
    /// matches the candidate's stack position; cells beyond the active count
    /// are simply set `isHidden = true`.
    private var chipPool: [ChipView] = []
    /// Cached cascade font — recomputed only when Dynamic Type changes.
    private var cachedFont: UIFont!

    override init(frame: CGRect) {
        super.init(frame: frame)
        setup()
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) not used") }

    private func setup() {
        backgroundColor = UIColor.systemBackground.withAlphaComponent(0.55)

        preeditLabel.font = .monospacedSystemFont(ofSize: 16, weight: .medium)
        preeditLabel.textColor = .label
        preeditLabel.text = ""
        preeditLabel.translatesAutoresizingMaskIntoConstraints = false
        preeditLabel.setContentHuggingPriority(.required, for: .horizontal)
        preeditLabel.setContentCompressionResistancePriority(.required, for: .horizontal)

        separator.backgroundColor = .separator
        separator.translatesAutoresizingMaskIntoConstraints = false

        scroll.showsHorizontalScrollIndicator = false
        scroll.alwaysBounceHorizontal = false
        scroll.decelerationRate = .fast    // snappier free-scroll feel (item 55)
        scroll.translatesAutoresizingMaskIntoConstraints = false
        scroll.delegate = self
        // VoiceOver — the scroll view itself reads as a scrollable container.
        scroll.accessibilityLabel = "候选区"
        scroll.accessibilityHint = "向左滑可查看更多候选"
        scroll.shouldGroupAccessibilityChildren = true

        stack.axis = .horizontal
        stack.alignment = .fill
        stack.distribution = .fill
        stack.spacing = 0
        stack.translatesAutoresizingMaskIntoConstraints = false

        // Item 56 counter — small, tertiary-label color, sits at trailing edge.
        countLabel.font = .systemFont(ofSize: 11, weight: .medium)
        countLabel.textColor = .tertiaryLabel
        countLabel.textAlignment = .right
        countLabel.text = ""
        countLabel.isHidden = true
        countLabel.translatesAutoresizingMaskIntoConstraints = false
        countLabel.setContentHuggingPriority(.required, for: .horizontal)
        countLabel.setContentCompressionResistancePriority(.required, for: .horizontal)
        countLabel.isAccessibilityElement = false   // duplicates scroll's accessibility hint

        scroll.addSubview(stack)
        addSubview(preeditLabel)
        addSubview(separator)
        addSubview(scroll)
        addSubview(countLabel)

        NSLayoutConstraint.activate([
            preeditLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 12),
            preeditLabel.centerYAnchor.constraint(equalTo: centerYAnchor),

            separator.leadingAnchor.constraint(equalTo: preeditLabel.trailingAnchor, constant: 8),
            separator.centerYAnchor.constraint(equalTo: centerYAnchor),
            separator.widthAnchor.constraint(equalToConstant: 1),
            separator.heightAnchor.constraint(equalTo: heightAnchor, multiplier: 0.55),

            scroll.leadingAnchor.constraint(equalTo: separator.trailingAnchor, constant: 4),
            scroll.trailingAnchor.constraint(equalTo: countLabel.leadingAnchor, constant: -4),
            scroll.topAnchor.constraint(equalTo: topAnchor),
            scroll.bottomAnchor.constraint(equalTo: bottomAnchor),

            stack.leadingAnchor.constraint(equalTo: scroll.leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: scroll.trailingAnchor),
            stack.topAnchor.constraint(equalTo: scroll.topAnchor),
            stack.bottomAnchor.constraint(equalTo: scroll.bottomAnchor),
            stack.heightAnchor.constraint(equalTo: scroll.heightAnchor),

            countLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),
            countLabel.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])

        cachedFont = Self.candidateFont(for: traitCollection)
    }

    /// Item 62 — when the user changes Dynamic Type in Settings, refresh
    /// the cached font and propagate to existing chips. Per-keystroke setState
    /// is no longer responsible for this.
    override func traitCollectionDidChange(_ previous: UITraitCollection?) {
        super.traitCollectionDidChange(previous)
        if previous?.preferredContentSizeCategory != traitCollection.preferredContentSizeCategory {
            cachedFont = Self.candidateFont(for: traitCollection)
            for chip in chipPool { chip.font = cachedFont }
        }
    }

    /// User preference forwarded from the keyboard. `false` hides the W/P
    /// dot for users who don't care which engine produced each candidate.
    /// Wired to App Group `UserDefaults.showSourceIndicator` in item 78.
    var showSourceIndicator: Bool = true

    /// Update the bar with the new preedit + candidate state. `sources`
    /// (parallel to `candidates`) carries the W/P attribution; pass `nil`
    /// to suppress dots entirely (e.g., bootstrap mode or wubi-only).
    func setState(preedit: String, candidates: [String], sources: [InputxCandidateSource?]? = nil) {
        #if DEBUG
        let t0 = CFAbsoluteTimeGetCurrent()
        #endif
        preeditLabel.text = preedit
        separator.isHidden = preedit.isEmpty || candidates.isEmpty
        lastCandidates = candidates
        lastSources = sources ?? Array(repeating: nil, count: candidates.count)
        #if DEBUG
        let tHeader = CFAbsoluteTimeGetCurrent()
        #endif

        ensureChipPool(capacity: candidates.count)
        #if DEBUG
        let tPool = CFAbsoluteTimeGetCurrent()
        #endif

        for (i, chip) in chipPool.enumerated() {
            if i < candidates.count {
                let src = lastSources[safe: i] ?? nil
                let label = Self.accessibilityLabel(for: candidates[i], source: src, index: i)
                chip.update(
                    text: candidates[i],
                    source: src,
                    index: i,
                    showSourceDot: showSourceIndicator,
                    accessibilityLabel: label
                )
                chip.isHidden = false
            } else {
                chip.isHidden = true
            }
        }
        #if DEBUG
        let tUpdate = CFAbsoluteTimeGetCurrent()
        #endif

        scroll.contentOffset = .zero
        updateCountLabel(visibleStart: 1)
        #if DEBUG
        let tDone = CFAbsoluteTimeGetCurrent()

        NSLog("[inputxperf]     setState header=%.2fms pool=%.2fms update=%.2fms scroll=%.2fms total=%.2fms",
              (tHeader - t0) * 1000,
              (tPool - tHeader) * 1000,
              (tUpdate - tPool) * 1000,
              (tDone - tUpdate) * 1000,
              (tDone - t0) * 1000)
        #endif
    }

    /// Eagerly create up to `count` chips so the first big candidate refresh
    /// doesn't pay allocator cost during a keystroke (visible as the lag
    /// spike on the first `kp`-style input that explodes the candidate set).
    /// Caller should pass the same cap they use per-refresh.
    func preallocateChips(_ count: Int) {
        ensureChipPool(capacity: count)
    }

    /// Force CoreText glyph measurement up front by stamping a CJK string
    /// into every chip and triggering a layout pass. Without this, the first
    /// real setState pays Pingfang + Lab8CJKExtended cascade-font glyph
    /// metrics for every chip during the keystroke — visible as a stutter
    /// the first time pinyin candidates appear.
    func warmChipLayout() {
        for chip in chipPool {
            chip.update(text: "字", source: nil, index: 0,
                        showSourceDot: false, accessibilityLabel: "")
            chip.isHidden = false
        }
        setNeedsLayout()
        layoutIfNeeded()
        for chip in chipPool { chip.isHidden = true }
    }

    /// Grow the pool up to `capacity`. Never shrinks — the cost of building
    /// a chip is paid at most once per slot across the keyboard's lifetime.
    private func ensureChipPool(capacity: Int) {
        while chipPool.count < capacity {
            let chip = ChipView()
            chip.font = cachedFont
            chip.onTap = { [weak self] idx in
                self?.onCandidateTap?(idx)
            }
            chip.onLongPress = { [weak self] idx, anchor in
                self?.handleChipLongPress(index: idx, anchor: anchor)
            }
            stack.addArrangedSubview(chip)
            chipPool.append(chip)
        }
    }

    // MARK: - Item 59: long-press → Unihan info popup

    private func handleChipLongPress(index: Int, anchor: UIView) {
        guard index >= 0, index < lastCandidates.count else { return }
        // Defer to host (KeyboardViewController) if it wants to override —
        // it has access to the engine for richer info (alt pinyin readings,
        // L0 status, etc.). Default = built-in codepoint popup.
        if let handler = onCandidateLongPress {
            handler(index, anchor)
            return
        }
        showDefaultInfoPopup(forIndex: index, anchor: anchor)
    }

    private func showDefaultInfoPopup(forIndex index: Int, anchor: UIView) {
        let word = lastCandidates[index]
        let source = lastSources[safe: index] ?? nil
        let info = Self.defaultInfoText(for: word, source: source)

        let alert = UIAlertController(title: word, message: info, preferredStyle: .actionSheet)
        alert.addAction(UIAlertAction(title: "好", style: .cancel))
        if let pop = alert.popoverPresentationController {
            pop.sourceView = anchor
            pop.sourceRect = anchor.bounds
        }
        // Walk the responder chain for a presenter — the chip's
        // viewController is the keyboard input controller.
        var responder: UIResponder? = self
        while responder != nil {
            if let vc = responder as? UIViewController {
                vc.present(alert, animated: true)
                return
            }
            responder = responder?.next
        }
    }

    private static func defaultInfoText(for word: String, source: InputxCandidateSource?) -> String {
        var lines: [String] = []
        for c in word {
            let cp = c.unicodeScalars.map { String(format: "U+%04X", $0.value) }.joined(separator: " + ")
            lines.append("\(c) — \(cp)")
        }
        if let s = source {
            let tag = s == .wubi ? "五笔" : "拼音"
            lines.append("")
            lines.append("来源:\(tag)")
        }
        return lines.joined(separator: "\n")
    }

    // MARK: - Helpers

    private static func accessibilityLabel(
        for word: String,
        source: InputxCandidateSource?,
        index: Int
    ) -> String {
        let position = "候选 \(index + 1)"
        let engineTag: String
        switch source {
        case .wubi:    engineTag = "五笔"
        case .pinyin:  engineTag = "拼音"
        case .unknown: engineTag = ""
        case .none:    engineTag = ""
        }
        if engineTag.isEmpty {
            return "\(position): \(word)"
        }
        return "\(position): \(word), \(engineTag)"
    }

    /// Update the trailing "N / total" count. Hidden if the chip stack
    /// fits inside the visible scroll bounds (no scrolling needed).
    private func updateCountLabel(visibleStart: Int) {
        let total = lastCandidates.count
        guard total > 0 else {
            countLabel.text = ""
            countLabel.isHidden = true
            return
        }
        // Best-effort visible-range estimation. Without per-chip bounds we
        // approximate: if scroll's content overflows, show "N / total" where
        // N = approximate visible count via scroll offset ratio.
        let needsScroll = stack.bounds.width > scroll.bounds.width + 0.5
        if !needsScroll {
            countLabel.text = "\(total)"
        } else {
            countLabel.text = "\(visibleStart) / \(total)"
        }
        countLabel.isHidden = false
    }
}

// MARK: - UIScrollViewDelegate (item 56 visible-range update)

extension CandidateBar: UIScrollViewDelegate {
    func scrollViewDidScroll(_ scrollView: UIScrollView) {
        // Estimate the leftmost visible chip index by walking arranged
        // subviews until one's frame.maxX exceeds the scroll's leading edge.
        let leftEdge = scrollView.contentOffset.x
        var firstVisible = 1
        for (i, v) in stack.arrangedSubviews.enumerated() {
            if v.isHidden { continue }
            if v.frame.maxX > leftEdge + 1 {
                firstVisible = i + 1
                break
            }
        }
        updateCountLabel(visibleStart: firstVisible)
    }
}

// Safe-subscript helper for parallel arrays where lengths may diverge by
// 1-2 elements during async refreshes.
private extension Array {
    subscript(safe i: Int) -> Element? {
        return (0..<count).contains(i) ? self[i] : nil
    }
}
