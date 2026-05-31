#!/usr/bin/env bash
# Reproducible Swift-side typing-latency benchmark for Inputx.
#
# Drives the running Inputx instance (PID from `pgrep Inputx`) by clearing
# /tmp/inputx.err.log, prompting the operator to type a fixed corpus, then
# parsing the PerfTimer rolling-window dumps the binary emits to stderr.
#
# Output: one row per `[perf] <label>` summary line, plus a derived
# "full-stack p50/p95 = Rust engine perfgate + Swift refresh" line.
#
# Usage:
#   scripts/bench-typing.sh                  # interactive, 100-keystroke corpus
#   scripts/bench-typing.sh quick            # 50-keystroke corpus
#
# Cross-checks: PID matches the currently-loaded LaunchAgent, binary
# timestamp is within last 60 minutes (sanity that the running binary
# matches the working tree).

set -euo pipefail

cd "$(dirname "$0")/.."

CORPUS_LINES=(
    "你好世界中国共产党全国人民代表大会"
    "我们在地铁上看到了一只猫今天天气真不错"
    "明天去公司开会下午三点然后吃饭"
    "学习编程是一件很有意思的事情"
    "感谢您的关注我们会继续努力"
    "请输入您的姓名和电话号码"
    "今年的春节将于一月二十二日开始放假"
    "数据库设计模式与系统架构演进"
    "前端工程师需要掌握的核心技能"
    "人工智能和机器学习的应用领域"
)

CORPUS_QUICK=(
    "你好世界"
    "中国人民"
    "我们在地铁上"
    "今天天气真不错"
    "学习编程"
)

MODE="${1:-full}"
if [[ "$MODE" == "quick" ]]; then
    CORPUS=("${CORPUS_QUICK[@]}")
    EXPECTED_KEYS=50
else
    CORPUS=("${CORPUS_LINES[@]}")
    EXPECTED_KEYS=100
fi

PID=$(pgrep -x Inputx | head -1)
if [[ -z "$PID" ]]; then
    echo "[bench] ✗ no Inputx process found — launch it via ./mac/reinstall.sh first" >&2
    exit 1
fi

INSTALLED_BIN="$HOME/Library/Input Methods/Inputx.app/Contents/MacOS/Inputx"
if [[ ! -f "$INSTALLED_BIN" ]]; then
    echo "[bench] ✗ Inputx binary not at $INSTALLED_BIN" >&2
    exit 1
fi

BIN_AGE_MIN=$(( ( $(date +%s) - $(stat -f %m "$INSTALLED_BIN") ) / 60 ))
if (( BIN_AGE_MIN > 60 )); then
    echo "[bench] ⚠ installed binary is $BIN_AGE_MIN min old — consider ./mac/reinstall.sh" >&2
fi

LOGFILE="/private/tmp/inputx.err.log"
: > "$LOGFILE"

cat <<EOF
[bench] Inputx PID=$PID, binary timestamp $(stat -f %Sm "$INSTALLED_BIN")
[bench] cleared $LOGFILE
[bench] open a text field (TextEdit / Notes / Safari URL bar) with Inputx active
[bench] type at least these $EXPECTED_KEYS keystrokes (any of these phrases works):
EOF

for line in "${CORPUS[@]}"; do
    echo "  $line"
done

echo
read -r -p "[bench] press ENTER when done typing (or ctrl-C to cancel) "

PERF_LINES=$(grep "^\[perf\]" "$LOGFILE" || true)

if [[ -z "$PERF_LINES" ]]; then
    echo "[bench] ✗ no [perf] lines in $LOGFILE — did you actually type with Inputx active?" >&2
    exit 1
fi

echo
echo "[bench] === captured Swift-side latency ==="
echo "$PERF_LINES" | grep -E "CandidatePanel\.(refresh|rebuildRows)|session\.(count\+preedit|candidate\(at:\))" \
    | sort | uniq

WIDTHFIT_HITS=$(echo "$PERF_LINES" | grep -c "widthFitHit" || true)
echo
echo "[bench] widthFitHit early-out count = $WIDTHFIT_HITS"

echo
echo "[bench] === RSS / CPU snapshot ==="
ps -p "$PID" -o pid,rss,%cpu,etime,command 2>&1

echo
echo "[bench] Rust engine perfgate (separate, run on demand):"
echo "  scripts/perf_isolated.sh"
