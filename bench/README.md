# Inputx Performance Benchmarks

Reproducible measurement points for evaluating Inputx against other macOS
input methods. All numbers below are wall-clock per single keystroke,
captured with the methodology described in each section.

## Quick reference (2026-05-31, develop @ a95d39f, post L1)

| Metric | Inputx | Apple Pinyin | Sogou (3rd-party reports) | vChewing |
|---|---|---|---|---|
| Keystroke latency p50 (Swift refresh) | **0.95 ms** | ~5-10 ms | ~5-15 ms | ~3-8 ms |
| Keystroke latency p95 (Swift refresh) | **3.2 ms** | ~10-15 ms | ~15-30 ms | ~10-15 ms |
| Full handler p50 (IMEController.handle) | **1.67 ms** | n/a | n/a | n/a |
| Full handler p95 (IMEController.handle) | **5.5 ms** | n/a | n/a | n/a |
| IMK→Swift dispatch p95 | **0.20 ms** | n/a (macOS limit) | n/a | n/a |
| Rust engine refresh_candidates p95 | **0.18 ms** | n/a (Apple) | n/a | n/a |
| Idle CPU | **0.0 %** | 0 % | 0-1 % | 0 % |
| Phys footprint (vmmap, fully warm) | **53.5 MB** (peak 76 MB) | ~30-50 MB | ~80-100 MB | ~30-40 MB |
| Cold-start lag on first keystroke | none (eager pre-warm + glyph cache fill) | minor | minor | none |
| Switch-to-IME cold lag | ~30 ms host-IMK-client first-IPC (dev-only after reinstall; production stays warm via launchd KeepAlive) | similar | similar | similar |
| IO/mem pressure sensitivity | hardened (warmup pre-faults all dict pages + ngram ctx_index pre-built) | unknown | reportedly degrades | unknown |

Comparison numbers are best-effort third-party estimates as of 2026-05;
absolute values vary by hardware and macOS version. The Inputx column is
measured locally on M1 / macOS 26.5.

**Note on memory**: earlier drafts of this doc reported 146 MB from
`ps -o rss`, which counts shared framework pages mmap'd from disk.
`vmmap -summary` is the correct measurement — it reports
`Physical footprint`, which is what Activity Monitor's "Memory"
column displays. Inputx settles at 53.5 MB after warmup with a 76 MB
peak during one-time init (ngram ctx_index HashMap build + glyph
cache pre-fill).

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

### 3. Memory / CPU snapshot

```sh
# Phys footprint (what Activity Monitor shows):
vmmap -summary $(pgrep -x Inputx) | grep "Physical footprint"

# CPU + elapsed:
ps -p $(pgrep -x Inputx) -o pid,%cpu,etime,command
```

Run after several minutes of typing to capture the warm steady state.
**Do not use `ps -o rss`** — it counts shared framework pages mmap'd
from disk and inflates the number 2-3×.

### 4. Stress test (typing under CPU pressure)

Verifies the IME doesn't degrade catastrophically when other apps are
hogging CPU. Spawns N `yes > /dev/null` background processes, asks the
operator to type during the pressure window, captures PerfTimer.

```sh
# 8 CPU stressors for 30 seconds (default):
scripts/bench-stress.sh

# Custom: 4 stressors for 60 seconds:
scripts/bench-stress.sh 4 60
```

Pass criteria (target):
- Refresh p50 stays within 2× of uncontended baseline (i.e., < 2 ms)
- Refresh p95 never exceeds one frame (16 ms)
- Handler p95 < 30 ms (host commitText IPC cold-tail is unavoidable)

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
| 5a5ba81 | 1.4 → 0.92 ms p50 (refresh); 17.85 → 5.04 ms max | alpha-toggle show/hide replaces orderFront (window-server compositor cost 3-9 ms gone); + L2 two-phase width measure; + IMK.dispatch instrumentation |
| a95d39f | 1.62 → 0.93 ms p50 (rebuildRows) | L1: CandidateRow internal AL constraints → manual `layout()` (kills AL re-propagate on stringValue changes) |
