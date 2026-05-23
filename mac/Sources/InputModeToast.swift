import Cocoa
import InputxKit

/// Brief HUD shown when the user toggles `InputxInputMode` via shift
/// single-click. Standard macOS HUD style — rounded translucent square
/// at screen center with a large glyph ("入" for CJK, "A" for EN),
/// shown for ~600ms then faded out over ~200ms.
///
/// Single-instance. IMK invokes `handle()` (and thus toggleInputMode)
/// on the main thread, which AppKit also requires for window mutations
/// — we rely on that runtime invariant rather than the type-system
/// `@MainActor` annotation, which the unannotated IMK callback chain
/// can't satisfy in Swift 6 strict concurrency.
final class InputModeToast {
    static let shared = InputModeToast()

    private var panel: ToastPanel?
    private var hideWorkItem: DispatchWorkItem?

    private init() {}

    /// Show the toast for the given mode, replacing any in-flight toast.
    func show(mode: InputxInputMode) {
        let glyph = (mode == .cjk) ? "入" : "A"
        let p = ensurePanel()
        p.setGlyph(glyph)
        p.centerOnActiveScreen()
        p.alphaValue = 1.0
        p.orderFrontRegardless()
        scheduleHide()
    }

    private func ensurePanel() -> ToastPanel {
        if let existing = panel { return existing }
        let p = ToastPanel()
        panel = p
        return p
    }

    private func scheduleHide() {
        hideWorkItem?.cancel()
        let item = DispatchWorkItem { [weak self] in
            guard let p = self?.panel else { return }
            NSAnimationContext.runAnimationGroup({ ctx in
                ctx.duration = 0.2
                p.animator().alphaValue = 0
            }, completionHandler: { [weak self] in
                // If another show() raced in during the fade, leave it visible.
                if (self?.panel?.alphaValue ?? 0) == 0 {
                    self?.panel?.orderOut(nil)
                }
            })
        }
        hideWorkItem = item
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.6, execute: item)
    }
}

/// Borderless non-activating panel with HUD-styled visual effect view +
/// centered large-glyph label. Never becomes key/main so the focused
/// host app keeps its keyboard focus across the flash.
private final class ToastPanel: NSPanel {
    private let label = NSTextField(labelWithString: "")

    init() {
        let size = NSSize(width: 160, height: 160)
        super.init(
            contentRect: NSRect(origin: .zero, size: size),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        isFloatingPanel = true
        level = .screenSaver
        isOpaque = false
        backgroundColor = .clear
        hasShadow = true
        ignoresMouseEvents = true
        collectionBehavior = [.canJoinAllSpaces, .stationary, .ignoresCycle]

        let visual = NSVisualEffectView(frame: NSRect(origin: .zero, size: size))
        visual.material = .hudWindow
        visual.blendingMode = .behindWindow
        visual.state = .active
        visual.wantsLayer = true
        visual.layer?.cornerRadius = 18
        visual.layer?.masksToBounds = true

        label.font = .systemFont(ofSize: 90, weight: .medium)
        label.alignment = .center
        label.textColor = .white
        label.translatesAutoresizingMaskIntoConstraints = false
        visual.addSubview(label)
        NSLayoutConstraint.activate([
            label.centerXAnchor.constraint(equalTo: visual.centerXAnchor),
            label.centerYAnchor.constraint(equalTo: visual.centerYAnchor),
        ])

        contentView = visual
    }

    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }

    func setGlyph(_ s: String) {
        label.stringValue = s
    }

    /// Center the panel on the screen containing the mouse cursor, or
    /// the main screen if that lookup fails. Multi-monitor users get the
    /// HUD on the screen they're currently looking at.
    func centerOnActiveScreen() {
        let mouse = NSEvent.mouseLocation
        let screen = NSScreen.screens.first { $0.frame.contains(mouse) } ?? NSScreen.main
        guard let frame = screen?.frame else { return }
        let panelFrame = self.frame
        let origin = NSPoint(
            x: frame.midX - panelFrame.width / 2,
            y: frame.midY - panelFrame.height / 2
        )
        setFrameOrigin(origin)
    }
}
