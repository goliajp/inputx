import Cocoa
import InputMethodKit
import InputxKit

/// Thin wrapper around `IMKCandidates` that lifts the bits of state IMKit
/// doesn't track for you: the current candidate list (so `commit(at:)` works
/// when the user picks a number key without ever opening the panel), and
/// the panel's anchor near the client's caret.
///
/// We use the **horizontal scrolling** panel style — closest visual match
/// to the iOS candidate bar and most macOS Chinese IMEs.
final class CandidatePanel {
    private let panel: IMKCandidates
    private weak var lastClient: AnyObject?

    /// Candidates currently displayed. Kept in sync with the engine session
    /// every time `refresh(...)` runs so the `numberKeyCommit` shortcut and
    /// `IMKCandidates` selection callbacks agree on indices.
    private(set) var current: [String] = []

    init(server: IMKServer) {
        // `kIMKScrollingGridCandidatePanel` would tile candidates in a grid;
        // we want a single horizontal strip like Sogou / 万能五笔, which is
        // what `kIMKSingleColumnScrollingCandidatePanel` provides when
        // combined with a horizontal layout via `setPanelType`.
        self.panel = IMKCandidates(
            server: server,
            panelType: kIMKSingleColumnScrollingCandidatePanel
        )
        self.panel.setSelectionKeysKeylayout(TISCopyCurrentKeyboardInputSource().takeRetainedValue())
        // 1-9 number-key shortcut for committing the corresponding candidate.
        // 0 deliberately skipped so it stays available as a plain digit.
        self.panel.setSelectionKeys([18, 19, 20, 21, 23, 22, 26, 28, 25])
        // Show source / next-prev hints when scrolling overflow exists.
        self.panel.setDismissesAutomatically(false)
    }

    /// Update the panel content to reflect the session's current candidates +
    /// preedit. Hides the panel if there's nothing to show.
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
        current = words
        panel.update()
        panel.setCandidateData(words as [Any])
        panel.show(kIMKLocateCandidatesBelowHint)
    }

    func hide() {
        current.removeAll(keepingCapacity: true)
        panel.hide()
    }

    /// Map a number key (1-9 on top row) to a candidate index. `nil` if the
    /// keycode isn't one of the configured selection keys or no candidate is
    /// at that index.
    func candidateIndex(forNumberKey codepoint: UInt32) -> Int? {
        // ASCII '1'…'9' → 0…8. '0' intentionally skipped (see init).
        guard codepoint >= 0x31, codepoint <= 0x39 else { return nil }
        let idx = Int(codepoint - 0x31)
        return idx < current.count ? idx : nil
    }

    var isVisible: Bool {
        // IMKCandidates doesn't expose isVisible; we proxy via candidate cache
        // (cleared on every hide).
        return !current.isEmpty
    }
}
