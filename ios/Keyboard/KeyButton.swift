import UIKit

enum KeyStyle {
    case letter            // light background, used for letters and most punctuation
    case modifier          // darker background, used for ⌫, 123, ABC, emoji, return
}

/// Single key button. Tap action via `onTap`. Haptic feedback differs by
/// style (item 66) — modifier keys (space/return/⌫) use heavier impact;
/// letter keys lighter. Long-press surfaces an optional variants popup
/// (items 63 + 64).
final class KeyButton: UIControl {
    var onTap: (() -> Void)?
    /// Variants surfaced on long-press (uppercase, accented, alt punct).
    /// Nil or empty disables the popup. The first variant string is what's
    /// inserted via `onVariantPick(_:)`.
    var variants: [String]? {
        didSet { configureLongPress() }
    }
    /// Called when the user long-presses + selects a variant.
    /// If `nil`, falls back to inserting the variant via `onTap` after
    /// temporarily replacing the button's title (the keyboard layer
    /// supplies a real wire-up via `KeyboardViewController`).
    var onVariantPick: ((String) -> Void)?

    private let label = UILabel()
    private let imageView = UIImageView()
    private let bg = UIView()
    let style: KeyStyle

    /// Lazily-created shared haptic generators per intensity tier.
    /// Item 66 — modifier (space/return) heavier, letters lighter.
    private static let lightImpact: UIImpactFeedbackGenerator = {
        let g = UIImpactFeedbackGenerator(style: .light)
        g.prepare()
        return g
    }()
    private static let heavyImpact: UIImpactFeedbackGenerator = {
        let g = UIImpactFeedbackGenerator(style: .medium)
        g.prepare()
        return g
    }()

    private var longPress: UILongPressGestureRecognizer?
    private weak var variantsPopup: VariantsPopupView?

    init(title: String, style: KeyStyle = .letter) {
        self.style = style
        super.init(frame: .zero)
        setupCommon()
        setupLabel(title: title)
        // Stable accessibility identifier for Maestro / XCUITest — survives
        // the lowercase↔uppercase letter-flip in shift mode (id stays "key-n"
        // whether the label says "n" or "N").
        accessibilityIdentifier = "key-\(title.lowercased())"
    }

    /// SF Symbol-based key (e.g. emoji face.smiling). Tint follows `.label`
    /// so the glyph adapts to light/dark mode like sibling 123/拼音 text keys.
    init(systemImage name: String, accessibilityLabel: String, style: KeyStyle = .modifier) {
        self.style = style
        super.init(frame: .zero)
        setupCommon()
        setupImage(systemImage: name)
        self.accessibilityLabel = accessibilityLabel
        accessibilityIdentifier = "key-\(accessibilityLabel.lowercased())"
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) not used") }

    private func setupCommon() {
        translatesAutoresizingMaskIntoConstraints = false

        bg.backgroundColor = restingBackground
        bg.layer.cornerRadius = 8
        bg.layer.cornerCurve = .continuous
        bg.isUserInteractionEnabled = false
        bg.translatesAutoresizingMaskIntoConstraints = false
        addSubview(bg)

        NSLayoutConstraint.activate([
            bg.topAnchor.constraint(equalTo: topAnchor),
            bg.bottomAnchor.constraint(equalTo: bottomAnchor),
            bg.leadingAnchor.constraint(equalTo: leadingAnchor),
            bg.trailingAnchor.constraint(equalTo: trailingAnchor),
        ])

        addTarget(self, action: #selector(handleTouchDown), for: .touchDown)
        addTarget(self, action: #selector(handleTouchUpInside), for: .touchUpInside)
        addTarget(self, action: #selector(handleTouchCancel), for: [.touchUpOutside, .touchCancel, .touchDragExit])

        // Item 61 — VoiceOver
        isAccessibilityElement = true
        accessibilityTraits = .keyboardKey
    }

    /// Center-alignment constraints kept around so a caller can swap to a
    /// non-centered layout (e.g. `alignLabelToBottomTrailing`) without
    /// having to rebuild the view.
    private var labelCenterX: NSLayoutConstraint?
    private var labelCenterY: NSLayoutConstraint?

    private func setupLabel(title: String) {
        label.text = title
        label.font = labelFont(for: title)
        label.textColor = .label
        label.textAlignment = .center
        label.adjustsFontSizeToFitWidth = true
        label.minimumScaleFactor = 0.7
        label.isUserInteractionEnabled = false
        label.translatesAutoresizingMaskIntoConstraints = false
        addSubview(label)

        let cx = label.centerXAnchor.constraint(equalTo: bg.centerXAnchor)
        let cy = label.centerYAnchor.constraint(equalTo: bg.centerYAnchor)
        labelCenterX = cx
        labelCenterY = cy
        NSLayoutConstraint.activate([
            cx,
            cy,
            label.leadingAnchor.constraint(greaterThanOrEqualTo: bg.leadingAnchor, constant: 2),
            label.trailingAnchor.constraint(lessThanOrEqualTo: bg.trailingAnchor, constant: -2),
        ])

        accessibilityLabel = title
    }

    /// Move the label to the bottom-right corner. Used by the space bar's
    /// "GOLIA" brand tag, which sits there in muted text — not a tappable
    /// label, just a quiet hint that this is Inputx / golia.jp's keyboard.
    func alignLabelToBottomTrailing(insetX: CGFloat = 10, insetY: CGFloat = 4) {
        labelCenterX?.isActive = false
        labelCenterY?.isActive = false
        label.textAlignment = .right
        NSLayoutConstraint.activate([
            label.trailingAnchor.constraint(equalTo: bg.trailingAnchor, constant: -insetX),
            label.bottomAnchor.constraint(equalTo: bg.bottomAnchor, constant: -insetY),
        ])
    }

    private func setupImage(systemImage name: String) {
        imageView.image = UIImage(systemName: name, withConfiguration: Self.iconConfig)
        imageView.tintColor = .label
        imageView.contentMode = .scaleAspectFit
        imageView.isUserInteractionEnabled = false
        imageView.translatesAutoresizingMaskIntoConstraints = false
        addSubview(imageView)

        // Fixed outer frame so SF Symbols with different intrinsic aspect
        // ratios (face.smiling = circle, shift = tall arrow, delete.left =
        // wide arrow) all sit inside the same visual box. Without this they
        // looked subtly off-balanced next to each other across row4.
        NSLayoutConstraint.activate([
            imageView.centerXAnchor.constraint(equalTo: bg.centerXAnchor),
            imageView.centerYAnchor.constraint(equalTo: bg.centerYAnchor),
            imageView.widthAnchor.constraint(equalToConstant: Self.iconBoxSize),
            imageView.heightAnchor.constraint(equalToConstant: Self.iconBoxSize),
        ])
    }

    /// Shared icon sizing — used by every SF Symbol key (shift, emoji,
    /// backspace, ...) so they're visually consistent.
    static let iconConfig = UIImage.SymbolConfiguration(
        pointSize: 20, weight: .regular, scale: .medium
    )
    static let iconBoxSize: CGFloat = 26

    func setTitle(_ title: String) {
        label.text = title
        label.font = labelFont(for: title)
        accessibilityLabel = title
    }

    /// Swap the SF Symbol used by a symbol-mode chip (e.g. shift ⇧ flipping
    /// between outline / fill / fill+underline as the shift state changes).
    /// No-op on label-mode buttons.
    func setSystemImage(_ name: String) {
        imageView.image = UIImage(systemName: name, withConfiguration: Self.iconConfig)
    }

    /// Override the label font (used by space's mode hint "五笔" which should
    /// read as a small subtle annotation, not a primary glyph).
    func setFontSize(_ size: CGFloat, weight: UIFont.Weight = .regular) {
        label.font = .systemFont(ofSize: size, weight: weight)
    }

    /// Override label text color (e.g. space's "拼" placeholder uses a
    /// secondary color so it reads as a mode hint, not a glyph the user
    /// would tap to insert).
    func setTextColor(_ color: UIColor) {
        label.textColor = color
    }

    // MARK: - Active state (item 89 — shift)

    /// `true` while shift is engaged (temp or locked). Flips the background
    /// and glyph color to a high-contrast "active" look — iOS's standard
    /// signal that the modifier is on.
    private(set) var isActiveState: Bool = false

    func setActive(_ active: Bool) {
        guard isActiveState != active else { return }
        isActiveState = active
        bg.backgroundColor = currentBackground
        // High-contrast glyph against the white-ish active bg.
        let activeFg: UIColor = .black
        label.textColor = active ? activeFg : .label
        imageView.tintColor = active ? activeFg : .label
    }

    /// Active-state background. Stays light on both light/dark mode (matches
    /// iOS shift's white-on-tap visual) but a touch dimmer in dark mode so
    /// it doesn't blow out next to the dark keys around it.
    private var activeBackground: UIColor {
        return UIColor { trait in
            trait.userInterfaceStyle == .dark
                ? UIColor(white: 0.92, alpha: 1.0)
                : UIColor.white
        }
    }

    private var currentBackground: UIColor {
        return isActiveState ? activeBackground : restingBackground
    }

    /// Restores semantic dark-mode-friendly colors. Item 68 — switched
    /// from fixed `UIColor(white:..)` to `UIColor(dynamicProvider:)` so
    /// resting/pressed backgrounds adapt to the system trait collection
    /// (light/dark) automatically.
    private var restingBackground: UIColor {
        switch style {
        case .letter:
            return UIColor { trait in
                trait.userInterfaceStyle == .dark
                    ? UIColor(white: 0.43, alpha: 1.0)   // dark = mid-gray
                    : UIColor(white: 0.99, alpha: 1.0)   // light = near-white
            }
        case .modifier:
            return UIColor { trait in
                trait.userInterfaceStyle == .dark
                    ? UIColor(white: 0.30, alpha: 1.0)
                    : UIColor(white: 0.74, alpha: 1.0)
            }
        }
    }

    private var pressedBackground: UIColor {
        switch style {
        case .letter:
            return UIColor { trait in
                trait.userInterfaceStyle == .dark
                    ? UIColor(white: 0.55, alpha: 1.0)
                    : UIColor(white: 0.82, alpha: 1.0)
            }
        case .modifier:
            return UIColor { trait in
                trait.userInterfaceStyle == .dark
                    ? UIColor(white: 0.45, alpha: 1.0)
                    : UIColor(white: 0.94, alpha: 1.0)
            }
        }
    }

    override func traitCollectionDidChange(_ previous: UITraitCollection?) {
        super.traitCollectionDidChange(previous)
        // Re-resolve dynamic colors when light/dark switches.
        if previous?.userInterfaceStyle != traitCollection.userInterfaceStyle {
            bg.backgroundColor = currentBackground
        }
    }

    private func labelFont(for title: String) -> UIFont {
        // Modifier keys all share one size regardless of character count,
        // so "五笔" / "拼音" (2 CJK chars) doesn't dwarf "123" / "ABC" /
        // "return" / "#+=" in the same row.
        if style == .modifier {
            return .systemFont(ofSize: 16, weight: .regular)
        }
        return .systemFont(ofSize: 22, weight: .regular)
    }

    private func configureLongPress() {
        if let existing = longPress {
            removeGestureRecognizer(existing)
            longPress = nil
        }
        guard let v = variants, !v.isEmpty else { return }
        let lp = UILongPressGestureRecognizer(target: self, action: #selector(handleLongPress(_:)))
        lp.minimumPressDuration = 0.4
        addGestureRecognizer(lp)
        longPress = lp
    }

    @objc private func handleLongPress(_ gr: UILongPressGestureRecognizer) {
        guard let v = variants, !v.isEmpty else { return }
        switch gr.state {
        case .began:
            heavyImpact()
            showVariantsPopup(v)
        case .changed:
            updatePopupHighlight(at: gr.location(in: self))
        case .ended:
            let pick = pickedVariant(at: gr.location(in: self))
            dismissVariantsPopup()
            if let pick = pick {
                onVariantPick?(pick) ?? onTap?()
                lightImpact()
            }
        case .cancelled, .failed:
            dismissVariantsPopup()
        default:
            break
        }
    }

    private func showVariantsPopup(_ variants: [String]) {
        guard variantsPopup == nil, let host = window else { return }
        let popup = VariantsPopupView(variants: variants)
        popup.translatesAutoresizingMaskIntoConstraints = false
        host.addSubview(popup)
        let frame = convert(bounds, to: host)
        // Center horizontally over the key, sit above it.
        let popupHeight: CGFloat = 56
        let popupWidth = max(CGFloat(variants.count) * 38 + 16, 56)
        var x = frame.midX - popupWidth / 2
        x = max(8, min(host.bounds.width - popupWidth - 8, x))
        let y = max(8, frame.minY - popupHeight - 4)
        popup.frame = CGRect(x: x, y: y, width: popupWidth, height: popupHeight)
        variantsPopup = popup
    }

    private func updatePopupHighlight(at point: CGPoint) {
        guard let popup = variantsPopup, let host = window else { return }
        let pointInHost = convert(point, to: host)
        popup.highlight(at: pointInHost)
    }

    private func pickedVariant(at point: CGPoint) -> String? {
        guard let popup = variantsPopup, let host = window else { return nil }
        let pointInHost = convert(point, to: host)
        return popup.variant(at: pointInHost)
    }

    private func dismissVariantsPopup() {
        variantsPopup?.removeFromSuperview()
        variantsPopup = nil
    }

    // MARK: Haptics — item 66 differentiation

    private func lightImpact() {
        Self.lightImpact.impactOccurred(intensity: 0.6)
    }

    private func heavyImpact() {
        Self.heavyImpact.impactOccurred(intensity: 1.0)
    }

    @objc private func handleTouchDown() {
        switch style {
        case .letter:   lightImpact()
        case .modifier: heavyImpact()
        }
        bg.backgroundColor = pressedBackground
    }

    @objc private func handleTouchUpInside() {
        bg.backgroundColor = currentBackground
        // If the long-press popup ate this gesture, the recognizer's .ended
        // already handled the variant selection and onTap should NOT fire.
        if variantsPopup != nil { return }
        onTap?()
    }

    @objc private func handleTouchCancel() {
        bg.backgroundColor = currentBackground
    }
}

// MARK: - VariantsPopupView (items 63 + 64)

/// Popup shown above a key during long-press. Renders a row of variant
/// chips; tracks finger position to highlight the active one. Borrowed
/// styling from the system iOS keyboard's letter-variants popover.
private final class VariantsPopupView: UIView {
    private let stack = UIStackView()
    private var chips: [UILabel] = []
    private(set) var variants: [String]
    private var highlightedIndex: Int?

    init(variants: [String]) {
        self.variants = variants
        super.init(frame: .zero)
        setup()
    }
    required init?(coder: NSCoder) { fatalError() }

    private func setup() {
        backgroundColor = UIColor { trait in
            trait.userInterfaceStyle == .dark
                ? UIColor(white: 0.20, alpha: 1.0)
                : UIColor(white: 0.97, alpha: 1.0)
        }
        layer.cornerRadius = 10
        layer.cornerCurve = .continuous
        layer.shadowColor = UIColor.black.cgColor
        layer.shadowOpacity = 0.18
        layer.shadowOffset = CGSize(width: 0, height: 2)
        layer.shadowRadius = 4

        stack.axis = .horizontal
        stack.alignment = .fill
        stack.distribution = .fillEqually
        stack.spacing = 0
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.topAnchor.constraint(equalTo: topAnchor, constant: 4),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -4),
            stack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),
        ])

        for v in variants {
            let lbl = UILabel()
            lbl.text = v
            lbl.font = .systemFont(ofSize: 22, weight: .regular)
            lbl.textAlignment = .center
            lbl.textColor = .label
            stack.addArrangedSubview(lbl)
            chips.append(lbl)
        }
    }

    func highlight(at pointInWindow: CGPoint) {
        var newIdx: Int? = nil
        for (i, chip) in chips.enumerated() {
            let chipFrame = chip.convert(chip.bounds, to: window)
            let extended = chipFrame.insetBy(dx: -4, dy: -16)
            if extended.contains(pointInWindow) {
                newIdx = i
                break
            }
        }
        if newIdx != highlightedIndex {
            for (i, chip) in chips.enumerated() {
                chip.layer.cornerRadius = 6
                if i == newIdx {
                    chip.backgroundColor = UIColor.systemBlue
                    chip.textColor = .white
                } else {
                    chip.backgroundColor = .clear
                    chip.textColor = .label
                }
            }
            highlightedIndex = newIdx
        }
    }

    func variant(at pointInWindow: CGPoint) -> String? {
        for (i, chip) in chips.enumerated() {
            let chipFrame = chip.convert(chip.bounds, to: window)
            let extended = chipFrame.insetBy(dx: -4, dy: -16)
            if extended.contains(pointInWindow) {
                return variants[i]
            }
        }
        // Release point isn't over any chip — fall back to the first
        // variant. Matches iOS-standard variant popup behavior: a
        // long-press without horizontal movement picks the first
        // (default) variant when the finger lifts. Also makes maestro
        // `longPressOn id: "key-X"` testable (XCUITest's synthetic gesture
        // releases at the press location, which is the key's center, not
        // over any popup chip).
        return variants.first
    }
}
