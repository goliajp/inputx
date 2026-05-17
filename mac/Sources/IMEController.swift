import Cocoa
import InputMethodKit

@objc(InputxController)
class InputxController: IMKInputController {
    private let session = InputxSession()

    override func handle(_ event: NSEvent!, client sender: Any!) -> Bool {
        guard let event = event, event.type == .keyDown else { return false }

        // Use charactersIgnoringModifiers so 'A' (Shift+a) still becomes 'a'
        // for Wubi letter routing. Modifier state is passed separately.
        guard let chars = event.charactersIgnoringModifiers,
              let firstScalar = chars.unicodeScalars.first
        else {
            return false
        }
        let codepoint = firstScalar.value

        // Skip Apple PUA (arrow keys, function keys, F1-F19) so they don't
        // get fed to the engine as bogus codepoints.
        if (0xF700...0xF8FF).contains(codepoint) {
            return false
        }

        let mods = mapModifiers(event.modifierFlags)

        let consumed = session.handleKey(codepoint: codepoint, modifiers: mods)
        if !consumed {
            return false
        }

        // 1. Drain pending commit (auto-commit / 5th-letter force / manual).
        if let committed = session.takeCommit(), !committed.isEmpty {
            commitText(committed, to: sender)
        }

        // 2. Update preedit (marked text in client). M2 candidate panel will
        // read candidate state here too.
        updatePreedit(client: sender)

        return true
    }

    override func deactivateServer(_ sender: Any!) {
        // Client switched away while composing — drop in-flight state rather
        // than auto-commit into a textfield the user just left.
        session.clear()
        clearMarkedText(client: sender)
        super.deactivateServer(sender)
    }

    private func mapModifiers(_ flags: NSEvent.ModifierFlags) -> InputxModifiers {
        var m: InputxModifiers = []
        if flags.contains(.shift)    { m.insert(.shift) }
        if flags.contains(.control)  { m.insert(.ctrl) }
        if flags.contains(.option)   { m.insert(.alt) }
        if flags.contains(.command)  { m.insert(.cmd) }
        if flags.contains(.function) { m.insert(.fn) }
        return m
    }

    private func commitText(_ text: String, to sender: Any?) {
        guard let client = sender as? IMKTextInput else { return }
        client.insertText(
            text,
            replacementRange: NSRange(location: NSNotFound, length: 0)
        )
    }

    private func updatePreedit(client sender: Any?) {
        guard let client = sender as? IMKTextInput else { return }
        if let preedit = session.preedit, !preedit.isEmpty {
            let attr = NSAttributedString(string: preedit)
            client.setMarkedText(
                attr,
                selectionRange: NSRange(location: preedit.count, length: 0),
                replacementRange: NSRange(location: NSNotFound, length: 0)
            )
        } else {
            clearMarkedText(client: sender)
        }
    }

    private func clearMarkedText(client sender: Any?) {
        guard let client = sender as? IMKTextInput else { return }
        client.setMarkedText(
            NSAttributedString(string: ""),
            selectionRange: NSRange(location: 0, length: 0),
            replacementRange: NSRange(location: NSNotFound, length: 0)
        )
    }
}
