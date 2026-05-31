// Fully automated perf+mem benchmark for Inputx.
//
// Pipeline:
//   1. Activate TextEdit, create a fresh empty document, focus it.
//   2. Select Inputx as the active input source via TIS API.
//   3. CGEventPost a fixed deterministic corpus of keystrokes — they
//      route through the IMK stack into the running Inputx process
//      exactly as if a human were typing.
//   4. Print "DONE" so the shell wrapper knows to collect metrics.
//
// Why CGEventPost (vs `osascript ... keystroke ...`): AppleScript's
// keystroke command often injects characters via TextInput services
// that bypass the active IME. CGEventPost simulates hardware keyDown
// events that flow through the same path as a real keyboard press,
// so Inputx's `handle:client:` is invoked exactly as in production.
//
// Requires accessibility permission for the terminal / IDE running
// this script — first run will prompt; subsequent runs are silent.
//
// Compile:  swiftc scripts/bench-auto.swift -o /tmp/bench-auto -framework Carbon -framework Cocoa
// Run:      /tmp/bench-auto

import Carbon
import Cocoa

// ──────────────────────────────────────────────────────────────────────
// Fixed corpus — 280 ASCII chars covering common pinyin + simplified
// initials + punctuation. Deterministic so cross-run comparisons are
// apples-to-apples.
// ──────────────────────────────────────────────────────────────────────
// Plain-pinyin corpus only — no 简拼 chains (wsm/tnnd) that leave
// buffers half-composed. Each word ends in a space so Inputx commits
// the top candidate cleanly; the final \n ensures any trailing buffer
// gets committed before measurement.
let CORPUS = """
nihao women zaiqu beijing jintian tianqi henhao \
xiawu sandian zai gongyuan jianmian \
wo shi yige zhongguoren women de zuguo \
shi shijieshang zui weida de guojia \
gongsi kaihui ranhou yiqi qu chifan \
zhege xiangmu hen zhongyao yao renzhen wancheng
"""

let KEY_CODES: [Character: CGKeyCode] = [
    "a": 0, "s": 1, "d": 2, "f": 3, "h": 4, "g": 5, "z": 6, "x": 7,
    "c": 8, "v": 9, "b": 11, "q": 12, "w": 13, "e": 14, "r": 15,
    "y": 16, "t": 17, "o": 31, "u": 32, "i": 34, "p": 35, "l": 37,
    "j": 38, "k": 40, "n": 45, "m": 46, " ": 49,
]

let SPACE_KEYCODE: CGKeyCode = 49
let RETURN_KEYCODE: CGKeyCode = 36

let INPUTX_SOURCE_ID = "jp.golia.inputmethod.wubi.zh"

// ──────────────────────────────────────────────────────────────────────
// 1. Open TextEdit + new document
// ──────────────────────────────────────────────────────────────────────
func activateTextEditWithFreshDoc() {
    // Force-quit any existing TextEdit so its IMK client is freshly
    // bound to whatever Inputx PID is currently active. A stale TextEdit
    // process holding an IMK connection to a dead-or-replaced Inputx
    // PID is the failure mode that left bench runs with empty
    // PerfTimer logs (host's keystrokes never reach our handle()).
    let quit = Process()
    quit.launchPath = "/usr/bin/osascript"
    quit.arguments = ["-e", "tell application \"TextEdit\" to quit saving no"]
    quit.standardOutput = FileHandle.nullDevice
    quit.standardError = FileHandle.nullDevice
    quit.launch()
    quit.waitUntilExit()
    Thread.sleep(forTimeInterval: 0.5)

    // Fresh launch.
    let activate = Process()
    activate.launchPath = "/usr/bin/open"
    activate.arguments = ["-a", "TextEdit"]
    activate.launch()
    activate.waitUntilExit()
    Thread.sleep(forTimeInterval: 0.5)

    let script = """
    tell application "TextEdit"
        activate
        make new document
    end tell
    """
    let osascript = Process()
    osascript.launchPath = "/usr/bin/osascript"
    osascript.arguments = ["-e", script]
    // Drop osascript's stdout (the document name); shell wrapper
    // expects bench-auto's stdout to be empty so JSON-from-bash is clean.
    osascript.standardOutput = FileHandle.nullDevice
    osascript.launch()
    osascript.waitUntilExit()
    Thread.sleep(forTimeInterval: 0.5)
    fputs("[bench-auto] TextEdit fresh doc focused\n", stderr)
}

// ──────────────────────────────────────────────────────────────────────
// 2. TIS — select Inputx as active input source
// ──────────────────────────────────────────────────────────────────────
func selectInputxInputSource() -> Bool {
    guard let raw = TISCreateInputSourceList(nil, false)?.takeRetainedValue() else {
        fputs("[bench-auto] ERR: TISCreateInputSourceList returned nil\n", stderr)
        return false
    }
    let sources = raw as NSArray as! [TISInputSource]
    for src in sources {
        let idPtr = TISGetInputSourceProperty(src, kTISPropertyInputSourceID)
        guard let idStr = idPtr.map({ Unmanaged<CFString>.fromOpaque($0).takeUnretainedValue() as String })
        else { continue }
        if idStr == INPUTX_SOURCE_ID {
            let status = TISSelectInputSource(src)
            if status == noErr {
                fputs("[bench-auto] selected Inputx as active input source\n", stderr)
                Thread.sleep(forTimeInterval: 0.5)
                return true
            } else {
                fputs("[bench-auto] ERR: TISSelectInputSource status=\(status)\n", stderr)
                return false
            }
        }
    }
    fputs("[bench-auto] ERR: Inputx input source (\(INPUTX_SOURCE_ID)) not in installed list\n", stderr)
    return false
}

// ──────────────────────────────────────────────────────────────────────
// 3. CGEventPost the corpus, key by key
// ──────────────────────────────────────────────────────────────────────
func sendCorpus(_ s: String, interKeyDelayMicros: useconds_t = 25_000) {
    let src = CGEventSource(stateID: .hidSystemState)
    var sent = 0
    var skipped = 0
    for ch in s.lowercased() {
        let kc: CGKeyCode
        if ch == "\n" {
            kc = RETURN_KEYCODE
        } else if let mapped = KEY_CODES[ch] {
            kc = mapped
        } else {
            skipped += 1
            continue
        }
        let down = CGEvent(keyboardEventSource: src, virtualKey: kc, keyDown: true)
        let up = CGEvent(keyboardEventSource: src, virtualKey: kc, keyDown: false)
        down?.post(tap: .cghidEventTap)
        usleep(2_000) // 2ms between down and up
        up?.post(tap: .cghidEventTap)
        sent += 1
        usleep(interKeyDelayMicros)
        // After each word (space), give Inputx a moment to refresh + commit.
        if kc == SPACE_KEYCODE {
            usleep(20_000)
        }
    }
    fputs("[bench-auto] sent=\(sent) skipped=\(skipped)\n", stderr)
}

// ──────────────────────────────────────────────────────────────────────
// main
// ──────────────────────────────────────────────────────────────────────
fputs("[bench-auto] starting...\n", stderr)
activateTextEditWithFreshDoc()
guard selectInputxInputSource() else { exit(1) }
// Settle: let Inputx warmup finish if this is a fresh process.
Thread.sleep(forTimeInterval: 1.5)
sendCorpus(CORPUS)
// Final enter to commit any trailing buffer that wasn't terminated by
// a space (otherwise it stays as preedit and PerfTimer's refresh window
// doesn't fully drain).
do {
    let src = CGEventSource(stateID: .hidSystemState)
    let down = CGEvent(keyboardEventSource: src, virtualKey: RETURN_KEYCODE, keyDown: true)
    let up = CGEvent(keyboardEventSource: src, virtualKey: RETURN_KEYCODE, keyDown: false)
    down?.post(tap: .cghidEventTap)
    usleep(2_000)
    up?.post(tap: .cghidEventTap)
}
// Let any pending refresh / IPC drain so PerfTimer flushes.
Thread.sleep(forTimeInterval: 1.0)

// Cleanup: close all TextEdit documents without saving + quit the app.
// Leaves no "未命名 N" artifacts piling up across runs.
let cleanupScript = """
tell application "TextEdit"
    try
        close every window saving no
    end try
    quit saving no
end tell
"""
let cleanup = Process()
cleanup.launchPath = "/usr/bin/osascript"
cleanup.arguments = ["-e", cleanupScript]
cleanup.standardOutput = FileHandle.nullDevice
cleanup.standardError = FileHandle.nullDevice
cleanup.launch()
cleanup.waitUntilExit()
fputs("[bench-auto] done (TextEdit closed)\n", stderr)
