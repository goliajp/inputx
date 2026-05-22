#!/usr/bin/env python3
"""Inputx Dev WebServer — live engine introspection + data debugging.

A localhost HTTP server (stdlib only — no flask/fastapi) that exposes:

  GET  /                          → home page with engine probe
  GET  /api/probe?q=<buffer>&mode=mixed&jp=0
                                  → live candidate breakdown
  GET  /api/weights?q=<code>      → search pinyin weights.tsv by code prefix
  GET  /api/heteronyms            → dump heteronyms_curated.tsv
  GET  /api/polish-log            → recent + aggregated polish-log entries
  POST /api/rebuild               → trigger pinyin-build-fst + cargo
                                    (returns log; stub for safety, dev opts in)

Start:
    python3 tools/devserver/server.py
    open http://localhost:7878

The probe subprocesses the `inputx-probe` Rust binary (cargo-built in
release mode) so the dev UI sees the real engine, not a model.

Why stdlib http.server instead of Flask: zero install ceremony for any
developer cloning the repo; the surface area we need (file reads,
subprocess, JSON) is trivial and stdlib serves it fine.
"""

from __future__ import annotations

import argparse
import collections
import csv
import json
import os
import subprocess
import sys
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent  # repo root
PROBE_BIN = None  # populated at server start
WEIGHTS_PATH = ROOT / "core/crates/inputx-pinyin/data/weights/weights.tsv"
HETERONYMS_PATH = ROOT / "core/crates/inputx-pinyin/data/heteronyms_curated.tsv"
POLISH_LOG_PATH = (
    Path.home()
    / "Library/Containers/jp.golia.inputmethod.wubi/Data/Library/"
      "Application Support/Inputx/polish-log.jsonl"
)
STATIC_DIR = Path(__file__).resolve().parent / "static"


def find_probe_bin() -> Path:
    """Find the inputx-probe binary, building if needed."""
    cargo_target = os.environ.get("CARGO_TARGET_DIR")
    if not cargo_target:
        # Honor user's wrapper-managed external target dir if present.
        try:
            out = subprocess.run(
                ["cargo", "metadata", "--format-version", "1", "--no-deps"],
                check=True, capture_output=True, text=True,
                cwd=str(ROOT / "core"),
            )
            cargo_target = json.loads(out.stdout)["target_directory"]
        except Exception:
            cargo_target = str(ROOT / "core" / "target")
    probe = Path(cargo_target) / "release" / "inputx-probe"
    if not probe.exists():
        print(f"[devserver] inputx-probe not found at {probe}; building...")
        subprocess.run(
            ["cargo", "build", "--release", "-p", "inputx-core",
             "--bin", "inputx-probe"],
            cwd=str(ROOT / "core"),
            check=True,
        )
    return probe


# ----- API handlers --------------------------------------------------------

def api_probe(qs: dict[str, list[str]]) -> tuple[int, dict]:
    q = (qs.get("q") or [""])[0]
    mode = (qs.get("mode") or ["mixed"])[0]
    jp = (qs.get("jp") or ["0"])[0] in ("1", "true", "on")
    if not q:
        return 200, {"buffer": "", "candidates": [], "mode": mode,
                     "japaneseEnabled": jp, "preedit": ""}
    cmd = [str(PROBE_BIN), q, "--mode", mode]
    if jp:
        cmd.append("--jp")
    try:
        out = subprocess.run(cmd, check=True, capture_output=True,
                             text=True, timeout=10)
    except subprocess.CalledProcessError as e:
        return 500, {"error": "probe failed",
                     "stderr": e.stderr,
                     "code": e.returncode}
    except subprocess.TimeoutExpired:
        return 504, {"error": "probe timed out"}
    return 200, json.loads(out.stdout)


def api_weights(qs: dict[str, list[str]]) -> tuple[int, dict]:
    q = (qs.get("q") or [""])[0].strip().lower()
    limit = int((qs.get("limit") or ["100"])[0])
    if not q:
        return 200, {"q": "", "rows": [], "note": "give ?q=<pinyin_prefix>"}
    rows = []
    with WEIGHTS_PATH.open() as f:
        for line in f:
            line = line.rstrip("\n")
            if not line or line.startswith("#"):
                continue
            parts = line.split("\t")
            if len(parts) < 2:
                continue
            code = parts[0]
            if not code.startswith(q):
                continue
            rows.append({
                "code": parts[0],
                "word": parts[1],
                "freq": int(parts[2]) if len(parts) >= 3 and parts[2].isdigit() else 0,
            })
            if len(rows) >= limit:
                break
    # Sort: exact code first, then by freq desc within each code
    rows.sort(key=lambda r: (0 if r["code"] == q else 1, -r["freq"], r["code"]))
    return 200, {"q": q, "rows": rows, "limit": limit}


def api_heteronyms(_qs) -> tuple[int, dict]:
    rows = []
    with HETERONYMS_PATH.open() as f:
        for line in f:
            line = line.rstrip("\n")
            if not line or line.startswith("#"):
                continue
            parts = line.split("\t")
            if len(parts) < 2:
                continue
            rows.append({"phrase": parts[0], "canonical": parts[1]})
    return 200, {"count": len(rows), "rows": rows}


def api_polish_log(qs) -> tuple[int, dict]:
    if not POLISH_LOG_PATH.exists():
        return 200, {"entries": [], "aggregate": {},
                     "note": f"no polish-log at {POLISH_LOG_PATH}"}
    limit = int((qs.get("limit") or ["50"])[0])
    entries = []
    by_pair: dict[tuple[str, str], int] = collections.Counter()
    with POLISH_LOG_PATH.open() as f:
        for line in f:
            line = line.strip()
            if not line: continue
            try:
                e = json.loads(line)
            except json.JSONDecodeError:
                continue
            entries.append(e)
            key = (e.get("buffer", ""), e.get("pickedWord", ""))
            by_pair[key] += 1
    recent = entries[-limit:][::-1]
    aggregate = [
        {"buffer": b, "preferred": w, "count": n}
        for (b, w), n in sorted(by_pair.items(), key=lambda x: -x[1])
        if n >= 2
    ][:30]
    return 200, {
        "total": len(entries),
        "recent": recent,
        "aggregate": aggregate,
    }


ROUTES = {
    "/api/probe": api_probe,
    "/api/weights": api_weights,
    "/api/heteronyms": api_heteronyms,
    "/api/polish-log": api_polish_log,
}


# ----- HTTP layer ----------------------------------------------------------

class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt: str, *args) -> None:  # quieter logs
        sys.stderr.write(f"[devserver] {self.address_string()} - {fmt % args}\n")

    def do_GET(self):
        parsed = urllib.parse.urlparse(self.path)
        path = parsed.path
        qs = urllib.parse.parse_qs(parsed.query, keep_blank_values=True)

        if path in ROUTES:
            try:
                status, payload = ROUTES[path](qs)
            except Exception as e:
                status, payload = 500, {"error": str(e)}
            body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", "application/json; charset=utf-8")
            self.send_header("Cache-Control", "no-store")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return

        # Static files
        if path in ("/", ""):
            path = "/index.html"
        f = STATIC_DIR / path.lstrip("/")
        if not f.exists() or not f.is_file():
            self.send_error(404, f"not found: {path}")
            return
        ctype = {
            ".html": "text/html; charset=utf-8",
            ".js":   "application/javascript; charset=utf-8",
            ".css":  "text/css; charset=utf-8",
            ".json": "application/json; charset=utf-8",
        }.get(f.suffix, "application/octet-stream")
        body = f.read_bytes()
        self.send_response(200)
        self.send_header("Content-Type", ctype)
        self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def main() -> int:
    global PROBE_BIN
    p = argparse.ArgumentParser()
    p.add_argument("--port", type=int, default=7878)
    p.add_argument("--bind", default="127.0.0.1")
    args = p.parse_args()
    PROBE_BIN = find_probe_bin()
    print(f"[devserver] probe binary: {PROBE_BIN}")
    print(f"[devserver] serving on http://{args.bind}:{args.port}")
    with ThreadingHTTPServer((args.bind, args.port), Handler) as httpd:
        try:
            httpd.serve_forever()
        except KeyboardInterrupt:
            print("\n[devserver] stopped")
    return 0


if __name__ == "__main__":
    sys.exit(main())
