import Foundation

/// Detects "shift was pressed and released without any other key in
/// between" — i.e. a single shift tap. Used to toggle `InputxInputMode`
/// between `.cjk` and `.en`.
///
/// Pure event-sequence analyzer with no AppKit / UIKit dependency, so it
/// ships in InputxKit and can be exercised from both the macOS IMKit
/// path and (future) iOS hardware-keyboard paths. The host (NSEvent on
/// macOS, UIPress on iOS) maps platform events into the three observe
/// methods below; left and right shift are both recognized.
///
/// State machine:
///
///   armed_keyCode = nil            ← initial / after fire / after disarm
///       │
///       │ observeShiftFlagsChanged(kc, shiftDown=true)
///       ▼
///   armed_keyCode = kc             ← waiting to see if shift was alone
///       │
///       │ observeShiftFlagsChanged(kc, shiftDown=false)  → fire if kc matches
///       │ observeKeyDown()                               → disarm (not alone)
///       │ observeOtherModifierChange()                   → disarm
///       ▼
///   armed_keyCode = nil
public final class InputxShiftSingleClickDetector {
    public static let leftShiftKeyCode: UInt16 = 56
    public static let rightShiftKeyCode: UInt16 = 60

    private var armedKeyCode: UInt16? = nil

    public init() {}

    /// A flags-changed event arrived. `keyCode` is the NSEvent.keyCode
    /// (or equivalent); only 56 / 60 are recognized. `shiftDown` reflects
    /// whether the resulting modifier mask has `.shift` set —
    ///   true  → physical shift press,
    ///   false → physical shift release.
    /// Returns `true` iff this event completes a single-click sequence
    /// (caller should toggle the input mode).
    @discardableResult
    public func observeShiftFlagsChanged(keyCode: UInt16, shiftDown: Bool) -> Bool {
        let isShiftKeyCode = (keyCode == Self.leftShiftKeyCode
                              || keyCode == Self.rightShiftKeyCode)
        guard isShiftKeyCode else { return false }
        if shiftDown {
            armedKeyCode = keyCode
            return false
        } else {
            let fire = (armedKeyCode == keyCode)
            armedKeyCode = nil
            return fire
        }
    }

    /// A non-shift modifier (cmd/option/ctrl/fn) toggled. Disarms the
    /// detector so a stranded shift-up later doesn't fire spuriously
    /// (e.g. user does shift-down, then cmd-down — that's now a chord,
    /// not a single shift click).
    public func observeOtherModifierChange() {
        armedKeyCode = nil
    }

    /// A regular keyDown arrived. Disarms the detector — shift was not
    /// alone; any character press makes it a chord.
    public func observeKeyDown() {
        armedKeyCode = nil
    }

    /// Force reset (e.g. IMK deactivateServer with shift held).
    public func reset() {
        armedKeyCode = nil
    }

    /// True iff a shift key is currently armed (for tests / introspection).
    public var isArmed: Bool {
        armedKeyCode != nil
    }
}
