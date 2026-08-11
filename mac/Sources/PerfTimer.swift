import Foundation

/// Lightweight rolling-window perf logger. Use `PerfTimer.measure("label") { ... }`
/// to wrap a hot path; every `windowSize` (default 50) calls per label prints
/// one line to stderr with min / p50 / p95 / max in milliseconds.
///
/// Always-on by default. To silence at runtime: `defaults write
/// jp.golia.inputmethod.wubi perfTimerEnabled -bool NO` (re-read on next
/// IME launch). Zero allocation on the fast path apart from the timing
/// array append; the summary line is built only at flush points.
///
/// Surface point: this exists so we can answer "did the Swift candidate
/// panel rebuild actually take Y ms" rather than reading off `sample(1)`
/// stack counts. Stays in tree; flip off via UserDefaults if it ever
/// becomes noise.
enum PerfTimer {

    static let windowSize = 50
    static let enabled: Bool = {
        if let v = UserDefaults.standard.object(forKey: "perfTimerEnabled") as? Bool {
            return v
        }
        return true
    }()

    static func measure<T>(_ label: String, _ body: () -> T) -> T {
        guard enabled else { return body() }
        let t0 = CFAbsoluteTimeGetCurrent()
        let result = body()
        let elapsed = CFAbsoluteTimeGetCurrent() - t0
        record(label, ms: elapsed * 1000.0)
        return result
    }

    private static let queue = DispatchQueue(label: "jp.golia.inputmethod.perf")
    private static var buckets: [String: [Double]] = [:]

    /// Record a pre-measured duration into a label's rolling window.
    /// Use this when the elapsed time is captured outside of a
    /// `measure { }` closure — e.g., for `defer`-based handler-exit
    /// timing or for upstream-latency measurements where the start
    /// time comes from an external source (NSEvent.timestamp etc.).
    static func record(label: String, ms: Double) {
        guard enabled else { return }
        record(label, ms: ms)
    }

    private static func record(_ label: String, ms: Double) {
        queue.async {
            var arr = buckets[label] ?? []
            arr.append(ms)
            if arr.count >= windowSize {
                arr.sort()
                let min = arr.first!
                let p50 = arr[arr.count / 2]
                let p95 = arr[Int(Double(arr.count) * 0.95)]
                let max = arr.last!
                FileHandle.standardError.write(Data(
                    String(
                        format: "[perf] %@: min=%.2fms p50=%.2fms p95=%.2fms max=%.2fms (n=%d)\n",
                        label, min, p50, p95, max, arr.count
                    ).utf8
                ))
                arr.removeAll(keepingCapacity: true)
            }
            buckets[label] = arr
        }
    }
}
