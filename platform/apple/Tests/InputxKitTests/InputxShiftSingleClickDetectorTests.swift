import XCTest
@testable import InputxKit

final class InputxShiftSingleClickDetectorTests: XCTestCase {
    private let L = InputxShiftSingleClickDetector.leftShiftKeyCode
    private let R = InputxShiftSingleClickDetector.rightShiftKeyCode

    func testPureLeftShiftClickFires() {
        let d = InputxShiftSingleClickDetector()
        XCTAssertFalse(d.observeShiftFlagsChanged(keyCode: L, shiftDown: true))
        XCTAssertTrue(d.observeShiftFlagsChanged(keyCode: L, shiftDown: false))
        XCTAssertFalse(d.isArmed)
    }

    func testPureRightShiftClickFires() {
        let d = InputxShiftSingleClickDetector()
        XCTAssertFalse(d.observeShiftFlagsChanged(keyCode: R, shiftDown: true))
        XCTAssertTrue(d.observeShiftFlagsChanged(keyCode: R, shiftDown: false))
    }

    func testShiftThenKeyDownDoesNotFireOnRelease() {
        let d = InputxShiftSingleClickDetector()
        XCTAssertFalse(d.observeShiftFlagsChanged(keyCode: L, shiftDown: true))
        d.observeKeyDown() // user typed shift+A
        XCTAssertFalse(d.observeShiftFlagsChanged(keyCode: L, shiftDown: false))
    }

    func testShiftThenOtherModifierDoesNotFire() {
        let d = InputxShiftSingleClickDetector()
        XCTAssertFalse(d.observeShiftFlagsChanged(keyCode: L, shiftDown: true))
        d.observeOtherModifierChange() // e.g. cmd toggled
        XCTAssertFalse(d.observeShiftFlagsChanged(keyCode: L, shiftDown: false))
    }

    func testReleaseOnDifferentShiftDoesNotFire() {
        // Press left, release right — physical impossibility but defensive.
        // Left was armed; right release doesn't match armed keyCode.
        let d = InputxShiftSingleClickDetector()
        XCTAssertFalse(d.observeShiftFlagsChanged(keyCode: L, shiftDown: true))
        XCTAssertFalse(d.observeShiftFlagsChanged(keyCode: R, shiftDown: false))
        // Original armed state cleared after the mismatched release.
        XCTAssertFalse(d.isArmed)
    }

    func testNonShiftKeyCodeIgnored() {
        let d = InputxShiftSingleClickDetector()
        // 0 = 'a'; not a shift keyCode
        XCTAssertFalse(d.observeShiftFlagsChanged(keyCode: 0, shiftDown: true))
        XCTAssertFalse(d.isArmed)
    }

    func testTwoConsecutiveClicksBothFire() {
        let d = InputxShiftSingleClickDetector()
        _ = d.observeShiftFlagsChanged(keyCode: L, shiftDown: true)
        XCTAssertTrue(d.observeShiftFlagsChanged(keyCode: L, shiftDown: false))
        _ = d.observeShiftFlagsChanged(keyCode: L, shiftDown: true)
        XCTAssertTrue(d.observeShiftFlagsChanged(keyCode: L, shiftDown: false))
    }

    func testKeyDownAloneDoesNothing() {
        let d = InputxShiftSingleClickDetector()
        d.observeKeyDown()
        XCTAssertFalse(d.isArmed)
    }

    func testResetClearsArmedState() {
        let d = InputxShiftSingleClickDetector()
        _ = d.observeShiftFlagsChanged(keyCode: L, shiftDown: true)
        XCTAssertTrue(d.isArmed)
        d.reset()
        XCTAssertFalse(d.isArmed)
        // A subsequent release must NOT fire — state was cleared.
        XCTAssertFalse(d.observeShiftFlagsChanged(keyCode: L, shiftDown: false))
    }
}
