import UIKit

/// Narrow proxy over the bits of `UITextDocumentProxy` the Inputx keyboard
/// actually uses — `insertText`, `deleteBackward`, and
/// `documentContextBeforeInput`. Letting `KeyboardViewController` talk to a
/// `InputxTextProxy` instead of directly to `UIInputViewController
/// .textDocumentProxy` means we can inject a fake when running the
/// keyboard inside the main host app for in-app testing (item 92), where
/// the real text-document proxy is a keyboard-extension-only thing.
@MainActor
protocol InputxTextProxy: AnyObject {
    func insertText(_ text: String)
    func deleteBackward()
    var documentContextBeforeInput: String? { get }
}

/// Default proxy: forward everything to the real `textDocumentProxy` on
/// the wrapped input view controller. Used when Inputx keyboard runs as a
/// real keyboard extension (the production path).
@MainActor
final class TextDocumentProxyAdapter: InputxTextProxy {
    private weak var inputVC: UIInputViewController?

    init(_ inputVC: UIInputViewController) {
        self.inputVC = inputVC
    }

    func insertText(_ text: String) {
        inputVC?.textDocumentProxy.insertText(text)
    }
    func deleteBackward() {
        inputVC?.textDocumentProxy.deleteBackward()
    }
    var documentContextBeforeInput: String? {
        inputVC?.textDocumentProxy.documentContextBeforeInput
    }
}
