import XCTest
import InputxKit

/// End-to-end exercise of the InputxKit Swift FFI binding — drives the Rust
/// engine through the same surface IMEController uses, without any IMK /
/// AppKit dependency. Locks the keystroke-path behaviors that v1.1.0-α1/α2
/// got wrong before: CJK return raw-commit, Cjk→En raw-commit, EN passthrough.
final class InputxSessionIntegrationTests: XCTestCase {

    private func session() -> InputxSession {
        InputxSession()
    }

    /// Regression: gmww (full 4-letter code) must rank single-char 两
    /// above the phrase 两败俱伤. Without the full-code-single-char-wins
    /// rule in inputx-wubi's lookup sort, Auto-layer chars lose to
    /// Phrase-layer idioms via layer base weight alone.
    func testCjkGmwwTopCandidateIsLiang() {
        let s = session()
        s.setAutoCommitPolicy(.never)
        for cp in "gmww".unicodeScalars {
            _ = s.handleKey(codepoint: cp.value, modifiers: [])
        }
        XCTAssertGreaterThan(s.candidateCount, 0)
        XCTAssertEqual(s.candidate(at: 0), "两",
                       "gmww top candidate should be 两 (single char)")
    }

    /// CJK wubi pipeline still works: `khlg` + space → "中国". khlg is the
    /// canonical multi-candidate test code in the engine's own tests; using
    /// the same input keeps this test deterministic across data refreshes.
    func testCjkWubiCommitsTopCandidateOnSpace() {
        let s = session()
        s.setAutoCommitPolicy(.never)
        for cp in "khlg".unicodeScalars {
            _ = s.handleKey(codepoint: cp.value, modifiers: [])
        }
        XCTAssertGreaterThan(s.candidateCount, 0)
        let consumed = s.handleKey(codepoint: 0x20, modifiers: [])
        XCTAssertTrue(consumed, "space should be consumed while composing")
        XCTAssertEqual(s.takeCommit(), "中国")
        XCTAssertEqual(s.preedit ?? "", "")
    }

    /// CJK + non-empty preedit + return → commit the raw wubi code as ASCII
    /// (user signalled "not CJK after all"). Engine swallows the \r so the
    /// host app doesn't receive a newline.
    func testCjkReturnWithPreeditCommitsRawAscii() {
        let s = session()
        s.setAutoCommitPolicy(.never)
        for cp in "huil".unicodeScalars {
            _ = s.handleKey(codepoint: cp.value, modifiers: [])
        }
        XCTAssertEqual(s.preedit, "huil")
        let consumed = s.handleKey(codepoint: 0x0D, modifiers: [])
        XCTAssertTrue(consumed, "return should be consumed while composing")
        XCTAssertEqual(s.takeCommit(), "huil")
        XCTAssertEqual(s.preedit ?? "", "")
    }

    /// CJK + empty preedit + return → engine doesn't consume, host gets \r.
    func testCjkReturnWithoutPreeditPassesThrough() {
        let s = session()
        XCTAssertFalse(s.handleKey(codepoint: 0x0D, modifiers: []))
        XCTAssertNil(s.takeCommit())
    }

    /// CJK + non-empty preedit + tab → engine swallows it. Tab is reserved
    /// for future candidate page navigation; right now the only requirement
    /// is that the host doesn't get a stray \t inserted ahead of an
    /// inadvertently committed candidate.
    func testCjkTabWithPreeditIsSwallowed() {
        let s = session()
        s.setAutoCommitPolicy(.never)
        for cp in "jeg".unicodeScalars {
            _ = s.handleKey(codepoint: cp.value, modifiers: [])
        }
        let preeditBefore = s.preedit
        XCTAssertTrue(s.handleKey(codepoint: 0x09, modifiers: []),
                      "tab while composing must be consumed")
        XCTAssertNil(s.takeCommit())
        XCTAssertEqual(s.preedit, preeditBefore)
    }

    /// CJK + empty preedit + tab → passthrough, host inserts a tab.
    func testCjkTabWithoutPreeditPassesThrough() {
        let s = session()
        XCTAssertFalse(s.handleKey(codepoint: 0x09, modifiers: []))
        XCTAssertNil(s.takeCommit())
    }

    /// CJK + non-empty preedit + Cjk→En toggle → commit raw ASCII before
    /// switching. Same semantic as the return path.
    func testCjkToEnWithPreeditCommitsRawAscii() {
        let s = session()
        s.setAutoCommitPolicy(.never)
        for cp in "huil".unicodeScalars {
            _ = s.handleKey(codepoint: cp.value, modifiers: [])
        }
        XCTAssertEqual(s.preedit, "huil")
        let newMode = s.toggleInputMode()
        XCTAssertEqual(newMode, .en)
        XCTAssertEqual(s.takeCommit(), "huil")
        XCTAssertEqual(s.inputMode, .en)
        XCTAssertEqual(s.preedit ?? "", "")
    }

    /// EN mode is pure passthrough — every keystroke variety returns false,
    /// no commits accumulate, preedit stays empty. Engine is dormant.
    func testEnModePassesAllKeystrokesThrough() {
        let s = session()
        s.setInputMode(.en)
        for cp in "hello world! 12,3.45".unicodeScalars {
            XCTAssertFalse(s.handleKey(codepoint: cp.value, modifiers: []),
                           "EN mode should passthrough '\(cp)'")
        }
        XCTAssertFalse(s.handleKey(codepoint: 0x0D, modifiers: []))  // return
        XCTAssertFalse(s.handleKey(codepoint: 0x08, modifiers: []))  // backspace
        XCTAssertFalse(s.handleKey(codepoint: 0x1B, modifiers: []))  // escape
        XCTAssertFalse(s.handleKey(codepoint: 0x63, modifiers: .cmd))  // cmd-c
        XCTAssertFalse(s.handleKey(codepoint: 0x61, modifiers: .ctrl)) // ctrl-a
        XCTAssertNil(s.takeCommit())
        XCTAssertEqual(s.preedit ?? "", "")
        XCTAssertEqual(s.candidateCount, 0)
    }

    /// En→Cjk has nothing to drain (EN doesn't buffer). Pure state flip.
    func testEnToCjkIsPureFlipNoCommit() {
        let s = session()
        s.setInputMode(.en)
        for cp in "abc".unicodeScalars {
            _ = s.handleKey(codepoint: cp.value, modifiers: [])
        }
        s.setInputMode(.cjk)
        XCTAssertNil(s.takeCommit())
        XCTAssertEqual(s.inputMode, .cjk)
    }

    /// clear() resets to default CJK + empty preedit, regardless of prior mode.
    func testClearResetsToCjkDefault() {
        let s = session()
        s.setInputMode(.en)
        for cp in "abc".unicodeScalars {
            _ = s.handleKey(codepoint: cp.value, modifiers: [])
        }
        s.clear()
        XCTAssertEqual(s.inputMode, .cjk)
        XCTAssertEqual(s.preedit ?? "", "")
        XCTAssertNil(s.takeCommit())
    }
}
