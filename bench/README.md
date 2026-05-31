# Inputx Performance Benchmarks

Reproducible measurement points for evaluating Inputx against other macOS
input methods. All numbers below are wall-clock per single keystroke,
captured with the methodology described in each section.

## Quick reference (2026-05-31, develop @ d5b7628)

| Metric | Inputx | Apple Pinyin | Sogou (3rd-party reports) | vChewing |
|---|---|---|---|---|
| Keystroke latency p50 (Swift refresh) | **1.65 ms** | ~5-10 ms | ~5-15 ms | ~3-8 ms |
| Keystroke latency p95 (Swift refresh) | **6.07 ms** | ~10-15 ms | ~15-30 ms | ~10-15 ms |
| Rust engine refresh_candidates p95 | **0.18 ms** | n/a (Apple) | n/a | n/a |
| Idle CPU | **0.0 %** | 0 % | 0-1 % | 0 % |
| Resident memory (RSS, fully warm) | 146 MB | ~50 MB | ~100 MB | ~40 MB |
| Cold-start lag on first keystroke | none (warmed) | minor | minor | none |
| IO/mem pressure sensitivity | hardened (warmup pre-faults all dict pages, ngram ctx_index pre-built) | unknown | reportedly degrades | unknown |

Comparison numbers are best-effort third-party estimates as of 2026-05;
absolute values vary by hardware and macOS version. The Inputx column is
measured locally on M1 / macOS 26.5.

## How to reproduce

### 1. Swift candidate-panel benchmark

Drives the running Inputx instance with a fixed Chinese typing corpus,
captures the PerfTimer rolling-window dumps emitted to stderr.

```sh
# 1. Ensure Inputx is installed + running
./mac/reinstall.sh

# 2. Run the bench harness (interactive)
scripts/bench-typing.sh           # 100-keystroke corpus
scripts/bench-typing.sh quick     # 50-keystroke corpus

# 3. The harness prints out per-label p50/p95/max from /tmp/inputx.err.log
```

The harness verifies the installed binary is recent (warns if > 60 min
old) and the Inputx process is actually running. PerfTimer is always-on
by default in release builds; flip off via
`defaults write jp.golia.inputmethod.wubi perfTimerEnabled -bool NO`.

### 2. Rust engine isolated perfgate

Single-threaded, no other workspace crates running in parallel — the
honest perf gate for the inputx-core algorithm hot paths
(refresh_candidates, scoring, bigram lookup).

```sh
scripts/perf_isolated.sh
```

Asserts both p95 < 16 ms (one frame at 60 Hz) and min < 8 ms (no
contention-adjusted baseline regression). Fails the CI gate if either
budget is exceeded.

### 3. RSS / CPU snapshot

```sh
ps -p $(pgrep -x Inputx) -o pid,rss,%cpu,etime,command
```

Run after several minutes of typing to capture the warm steady state.

## Methodology notes

- **Latency definition**: wall-clock time from `InputxController.handle`
  entry to PerfTimer.measure block exit. Includes:
  - Rust engine `Session::handle_key` (the IME state-machine update)
  - Swift `CandidatePanel.refresh` (candidate fetch, row update, frame
    sizing, AppKit `setFrame` with `display: false`)
  - **Excludes**: IMK roundtrip (cross-process latency from `keyDown` →
    `handle:client:` invocation), host-app text insertion (`commitText`
    via `IMKTextInput.insertText`).
- **Sample size**: PerfTimer flushes a summary line every 50 calls per
  label. Each `bench-typing.sh` run typically produces 2-4 summary
  lines per label across the 100-keystroke corpus.
- **Reproducibility**: the corpus is fixed; the harness logs the binary
  timestamp + PID for cross-run comparison. Run `bench-typing.sh` twice
  back-to-back to bound run-to-run noise.

## History — major perf wins (descending)

| Commit | Δ | What |
|---|---|---|
| 7534b33 | 100 → 16 ms p95 | inputx-ngram O(N) linear scan → O(1) lazy ctx index |
| c1a6053 | 16 → 5.5 ms p50 | CandidatePanel view-recycling + content fingerprint early-out |
| f659bba | 5.5 → 3.8 ms p50 | setHighlighted idempotent + word-width cache + hand-rolled frame calc |
| cfbd1b4 | 3.8 → 1.4 ms p50 | window.setFrame switched to `display: false` |
| 6960c3e | (cold-start) | Rust warmup expanded for IO/mem-pressure hardening |
| 1170c7c | (cold-start) | CandidatePanel eager pre-warm + setFrame skip + glyph cache pre-fill |
