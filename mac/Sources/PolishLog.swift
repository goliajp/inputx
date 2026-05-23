import Foundation

/// Append-only telemetry for "the IME's #0 candidate wasn't what the
/// user picked" events. Every time a user commits a candidate via
/// number key (digit ≥ 2 → idx ≥ 1) or by clicking a non-top entry in
/// the candidate panel, we append one JSONL line capturing:
///
///   - timestamp (ISO-8601)
///   - input buffer at commit time
///   - top-N candidate list as the user saw it
///   - picked index (0-based)
///   - picked word
///   - engine mode + JP-enabled flag at the time
///
/// The file lives at
/// `~/Library/Containers/jp.golia.inputmethod.wubi/Data/Library/Application Support/Inputx/polish-log.jsonl`
/// (sandboxed bundle container — same dir L0 storage uses).
///
/// Intent: build up a corpus of *real-world miss cases*. The dev review
/// flow is "open the log, look for repeat misses, fix the underlying
/// ranking / data / engine bug, ship a regression test". Picking #0 via
/// space is the implicit "the IME got it right" signal so we skip
/// those — only divergences are logged.
enum PolishLog {

    /// One commit-event payload. Encoded as a single JSONL line.
    struct Entry: Codable {
        let ts: String
        let buffer: String
        let candidates: [String]
        let pickedIdx: Int
        let pickedWord: String
        let engineMode: UInt8
        let japaneseEnabled: Bool
    }

    /// Append an entry if `pickedIdx > 0`. Calls from the keystroke
    /// hot path are cheap when this gate fails (the common space-
    /// commit case): no allocation, no file I/O.
    static func recordIfMiss(
        buffer: String,
        candidates: [String],
        pickedIdx: Int,
        pickedWord: String,
        engineMode: UInt8,
        japaneseEnabled: Bool
    ) {
        guard pickedIdx > 0 else { return }
        let entry = Entry(
            ts: iso8601.string(from: Date()),
            buffer: buffer,
            candidates: Array(candidates.prefix(10)),
            pickedIdx: pickedIdx,
            pickedWord: pickedWord,
            engineMode: engineMode,
            japaneseEnabled: japaneseEnabled
        )
        appendJSONL(entry)
    }

    /// URL of the JSONL file. Caller uses for "reveal in Finder" /
    /// "open in editor" UI surfaces.
    static var url: URL {
        ensureDir()
        return logDir.appendingPathComponent("polish-log.jsonl")
    }

    private static let iso8601: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f
    }()

    private static var logDir: URL {
        let support = FileManager.default
            .urls(for: .applicationSupportDirectory, in: .userDomainMask)
            .first!
        return support.appendingPathComponent("Inputx", isDirectory: true)
    }

    private static func ensureDir() {
        try? FileManager.default.createDirectory(
            at: logDir,
            withIntermediateDirectories: true
        )
    }

    private static func appendJSONL(_ entry: Entry) {
        guard let data = try? JSONEncoder().encode(entry),
              let line = String(data: data, encoding: .utf8)
        else { return }
        let payload = (line + "\n").data(using: .utf8) ?? Data()
        let fileURL = url
        if let fh = try? FileHandle(forWritingTo: fileURL) {
            defer { try? fh.close() }
            try? fh.seekToEnd()
            try? fh.write(contentsOf: payload)
        } else {
            // File doesn't exist yet — create it.
            try? payload.write(to: fileURL, options: .atomic)
        }
    }
}
