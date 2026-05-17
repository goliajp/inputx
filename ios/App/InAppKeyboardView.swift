import SwiftUI
import UIKit

/// In-app keyboard for debug + maestro TDD (item 92).
///
/// Hosts a live `KeyboardViewController` inside the main host app and
/// routes its insert/delete operations into a SwiftUI `@Binding<String>`
/// via a fake `InputxTextProxy`. This sidesteps the iOS third-party-
/// keyboard activation gate that prevents XCUITest / maestro from
/// driving the real Inputx keyboard extension reliably (every test-
/// framework interaction with the simulator resets the active keyboard
/// to the system default).
///
/// Since both InputxApp and InputxKeyboard targets now compile the same
/// `KeyboardViewController` source, an in-app instance exercises the
/// exact same engine + UI code path the extension uses — what's tested
/// here = what users see in the real keyboard.
struct InAppKeyboardView: UIViewControllerRepresentable {
    @Binding var text: String

    func makeUIViewController(context: Context) -> KeyboardViewController {
        let vc = KeyboardViewController()
        let fake = FakeTextProxy()
        fake.onChange = { newText in
            // Sync update — the engine's writes come from button event
            // handlers (touchUpInside), well outside SwiftUI's view-update
            // pass, so direct binding mutation is safe. Going async added
            // race-prone latency that made maestro `assertNotVisible`
            // assertions flaky (state observed pre-binding-propagation).
            if self.text != newText {
                self.text = newText
            }
        }
        vc.setInputxProxy(fake)
        return vc
    }

    func updateUIViewController(_ uiViewController: KeyboardViewController, context: Context) {
        // No-op: the keyboard drives itself; we only observe (one-way
        // binding from keyboard → text). External edits to `$text` won't
        // sync back into the engine's composing state for v1 — TBD if
        // we ever need it.
    }
}

/// `InputxTextProxy` impl that buffers inserts/deletes in a `String` and
/// emits change events. Used by `InAppKeyboardView` to bridge the
/// keyboard's text-output ops into a SwiftUI binding.
@MainActor
final class FakeTextProxy: InputxTextProxy {
    private(set) var text: String = ""
    var onChange: ((String) -> Void)?

    func insertText(_ text: String) {
        self.text.append(text)
        onChange?(self.text)
    }

    func deleteBackward() {
        guard !text.isEmpty else { return }
        text.removeLast()
        onChange?(text)
    }

    /// Engine reads this for AutoCaps (currently no-op'd in Inputx since
    /// it's a Chinese IME) and the inline-preedit reconciliation paths.
    var documentContextBeforeInput: String? { text }
}
