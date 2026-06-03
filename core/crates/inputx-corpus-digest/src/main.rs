//! `corpus-digest` — Phase A MVP of the corpus 3-layer pipeline tool.
//!
//! Design: `.claude/PLAN-corpus-digest.md` (full spec, edge cases pre-
//! decided). Invariants this tool MUST protect:
//!
//!   I-1. polish > digested, always.  Library rows with source="polish"
//!        are never touched by ingest — they win all collisions.
//!
//!   I-2. library 自主性.  Upstream removing a row does NOT delete from
//!        library. Only polish D1 deletes; this tool never DELETEs.
//!
//!   I-3. 幂等.  Same source content (same sha256) re-ingested produces
//!        a no-op (no library write, no log entry).
//!
//!   I-4. 三层原子.  Either ALL THREE writes (library + digest_log +
//!        source_registry) succeed, or none of them do (best effort: we
//!        write to tempfile + atomic rename in dependency order so a
//!        crash mid-flight leaves an inconsistent — but detectable —
//!        state that `verify` can flag).
//!
//!   I-5. append-only event log.  digest_log.toml is never rewritten,
//!        only appended.  source_registry.toml is the rewriteable
//!        "current state" complement.
//!
//!   I-6. 源无关.  library rows carry `source ∈ {digested, polish}`
//!        only — never upstream identity. Provenance lives in
//!        digest_log entries, keyed by event_id.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// ─── Paths (anchored at workspace root via CARGO_MANIFEST_DIR) ──────
//
// CARGO_MANIFEST_DIR = .../core/crates/inputx-corpus-digest
// repo root           = .../core/crates/inputx-corpus-digest/../../..

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()  // crates/
        .parent().unwrap()  // core/
        .parent().unwrap()  // repo
        .to_path_buf()
}

fn registry_path() -> PathBuf {
    repo_root().join("tools/scoring/data/source_registry.toml")
}

fn digest_log_path() -> PathBuf {
    repo_root().join("tools/scoring/data/digest_log.toml")
}

fn library_path(engine: &str) -> PathBuf {
    repo_root().join(format!("core/crates/inputx-{engine}/data/library.tsv"))
}

fn garbage_filter_path() -> PathBuf {
    repo_root().join("tools/scoring/data/polish/corpus_garbage_filter_v1.tsv")
}

fn cache_dir() -> PathBuf {
    repo_root().join("tools/scoring/data/cache/corpus_digest")
}

// ─── source_registry.toml schema ────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
struct SourceRegistry {
    version: u32,
    #[serde(default, rename = "source")]
    sources: Vec<Source>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Source {
    source_id: String,
    engine: String,
    source_type: String,
    source_name: String,
    fetch_kind: String,
    fetch_url: String,
    current_version: String,
    current_sha256: String,
    last_event_id: String,
    last_ingested_at: String,
    current_row_count: u64,
    ingest_format: String,
    auto_update: bool,
    auto_fetch_schedule: String,
    status: String,
    notes: String,
}

fn load_registry() -> Result<SourceRegistry, String> {
    let p = registry_path();
    let raw = fs::read_to_string(&p)
        .map_err(|e| format!("read {}: {e}", p.display()))?;
    toml::from_str(&raw)
        .map_err(|e| format!("parse {}: {e}", p.display()))
}

// ─── digest_log.toml schema ─────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
struct DigestLog {
    version: u32,
    #[serde(default, rename = "event")]
    events: Vec<Event>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Event {
    event_id: String,
    engine: String,
    source_type: String,
    source_name: String,
    source_version: String,
    source_sha256: String,
    ingested_at: String,
    rows_added: u64,
    #[serde(default)]
    rows_updated: u64,
    rows_rejected: u64,
    #[serde(default)]
    library_sha256_after: String,
    notes: String,
}

fn load_log() -> Result<DigestLog, String> {
    let p = digest_log_path();
    let raw = fs::read_to_string(&p)
        .map_err(|e| format!("read {}: {e}", p.display()))?;
    toml::from_str(&raw)
        .map_err(|e| format!("parse {}: {e}", p.display()))
}

// ─── library.tsv schema ─────────────────────────────────────────────
//
// pinyin :  code \t word \t            freq \t source
// wubi   :  code \t word \t layer   \t freq \t source
// nihongo:  code \t word \t type    \t freq \t source
//
// PLAN-corpus-digest §1.1: `code\tword\t[engine-specific cols]\tfreq\tsource`.
// `extra` carries the engine-specific col (wubi layer / nihongo type);
// None for pinyin.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EngineKind { Pinyin, Wubi, Nihongo }

impl EngineKind {
    fn parse(s: &str) -> Result<Self, String> {
        match s {
            "pinyin"  => Ok(Self::Pinyin),
            "wubi"    => Ok(Self::Wubi),
            "nihongo" => Ok(Self::Nihongo),
            other     => Err(format!("unknown engine {other:?}")),
        }
    }
    fn name(&self) -> &'static str {
        match self { Self::Pinyin => "pinyin", Self::Wubi => "wubi", Self::Nihongo => "nihongo" }
    }
    /// How many tab-separated fields a library.tsv ROW has.
    fn library_col_count(&self) -> usize {
        match self { Self::Pinyin => 4, Self::Wubi => 5, Self::Nihongo => 5 }
    }
    /// How many tab-separated fields a simple_tsv ingest input has.
    /// Same shape as library minus the trailing `source` col (the tool
    /// fills source=digested itself).
    fn simple_tsv_col_count(&self) -> usize {
        self.library_col_count() - 1
    }
}

#[derive(Debug, Clone)]
struct LibraryRow {
    code: String,
    word: String,
    /// wubi: layer (0..=5); nihongo: "kanji"|"jukugo"; pinyin: None.
    extra: Option<String>,
    freq: u32,
    source: String,  // "digested" | "polish"
}

/// Parse a library.tsv file. Preserves header comments (returned verbatim
/// for re-emission); blank/comment lines outside the header are dropped.
struct ParsedLibrary {
    header: String,           // verbatim leading `#` / blank lines
    rows: Vec<LibraryRow>,
}

fn load_library(engine: EngineKind) -> Result<ParsedLibrary, String> {
    let p = library_path(engine.name());
    let raw = fs::read_to_string(&p)
        .map_err(|e| format!("read {}: {e}", p.display()))?;

    let want_cols = engine.library_col_count();
    let mut header = String::new();
    let mut in_header = true;
    let mut rows = Vec::new();

    for (lineno, line) in raw.lines().enumerate() {
        let trimmed = line.trim_end();
        if in_header {
            if trimmed.is_empty() || trimmed.starts_with('#') {
                header.push_str(line);
                header.push('\n');
                continue;
            }
            in_header = false;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            // Drop interior blank/comment rows (rare; safer than
            // re-emitting them at arbitrary positions).
            continue;
        }
        let parts: Vec<&str> = trimmed.split('\t').collect();
        if parts.len() < want_cols {
            return Err(format!(
                "library {}:line {}: expected ≥{want_cols} cols, got {}",
                p.display(), lineno + 1, parts.len()
            ));
        }
        let (extra, freq_idx, source_idx) = match engine {
            EngineKind::Pinyin => (None, 2, 3),
            EngineKind::Wubi | EngineKind::Nihongo => (Some(parts[2].to_string()), 3, 4),
        };
        let freq = parts[freq_idx].parse::<u32>().map_err(|e| {
            format!("library {}:line {}: bad freq {:?}: {e}",
                p.display(), lineno + 1, parts[freq_idx])
        })?;
        let source = parts[source_idx].to_string();
        if source != "digested" && source != "polish" {
            return Err(format!(
                "library {}:line {}: bad source {:?} (want digested|polish)",
                p.display(), lineno + 1, source
            ));
        }
        rows.push(LibraryRow {
            code: parts[0].to_string(),
            word: parts[1].to_string(),
            extra,
            freq,
            source,
        });
    }
    Ok(ParsedLibrary { header, rows })
}

// ─── garbage filter (D1 future-proof — code\tword\tdate[\t# notes]) ──

fn load_garbage_filter() -> Result<std::collections::HashSet<(String, String)>, String> {
    let p = garbage_filter_path();
    let raw = fs::read_to_string(&p)
        .map_err(|e| format!("read {}: {e}", p.display()))?;
    let mut set = std::collections::HashSet::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let mut parts = line.split('\t');
        let (Some(code), Some(word)) = (parts.next(), parts.next()) else { continue };
        set.insert((code.trim().to_string(), word.trim().to_string()));
    }
    Ok(set)
}

// ─── Parsers (ingest_format dispatch) ───────────────────────────────

/// One row produced by an upstream parser, before STAGE 3+ normalization.
type IngestTuple = (String, String, Option<String>, u32);

/// Outcome of a parser run: rows it produced + rows it had to drop.
/// Drop count is reported to the user — silent truncation would let
/// upstream noise erode coverage without a signal.
struct ParseResult {
    rows: Vec<IngestTuple>,
    /// Rows the parser dropped (non-Han chars, freq parse fail, etc.).
    /// We surface the count and (in the jieba case) a sample, NOT silently.
    dropped: u64,
}

/// `simple_tsv`: each line matches the target library's row shape minus
/// the trailing `source` col.  For pinyin that's `code\tword\tfreq`
/// (3 cols); for wubi `code\tword\tlayer\tfreq` (4); for nihongo
/// `code\tword\ttype\tfreq` (4).
///
/// Blank + `#` lines skipped. Returns the raw tuples — no normalization
/// or dedup; STAGE 3+4 handle that.
fn parse_simple_tsv(raw: &str, engine: EngineKind) -> Result<ParseResult, String> {
    let want = engine.simple_tsv_col_count();
    let mut rows = Vec::with_capacity(1024);
    for (i, line) in raw.lines().enumerate() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') { continue; }
        let parts: Vec<&str> = t.split('\t').collect();
        if parts.len() < want {
            return Err(format!(
                "simple_tsv line {}: need {} cols for engine={}, got {}",
                i + 1, want, engine.name(), parts.len()
            ));
        }
        let (extra, freq_idx) = match engine {
            EngineKind::Pinyin => (None, 2),
            EngineKind::Wubi | EngineKind::Nihongo => (Some(parts[2].to_string()), 3),
        };
        let freq = parts[freq_idx].parse::<u32>()
            .map_err(|e| format!("simple_tsv line {}: bad freq {:?}: {e}", i + 1, parts[freq_idx]))?;
        rows.push((parts[0].to_string(), parts[1].to_string(), extra, freq));
    }
    Ok(ParseResult { rows, dropped: 0 })
}

/// `jieba_phrase`: jieba's `dict.txt` format — `<word> <freq> <pos>`
/// per line, whitespace-separated. We discard `<pos>`; freq becomes the
/// row's freq.  PINYIN ENGINE ONLY.
///
/// READING DERIVATION — "safe reading only":
///
/// jieba carries no pinyin reading.  We derive code char-by-char via
/// `char_to_pinyin`. CRITICAL: only emit the row when EVERY char has
/// exactly ONE reading in our reverse index. Otherwise — if any char
/// is 多音字 (multi-reading) — drop the row.
///
/// This is the lesson from the failed Phase B-3c apply:
///   - "暖和" → 暖(nuan) + 和({he, huo, …}) → first-reading heuristic
///     picked 和=he → derived "nuanhe" → wrong (canonical is "nuanhuo")
///   - Wrong (code, word) row entered the library + broke the
///     `lookup_nuanhe_does_not_return_nuanhuo` regression test.
///
/// So the rule is: if we can't be confident in the reading, we don't
/// ingest the word.  Multi-reading words need a parser that carries
/// reading metadata (cc_cedict, opencc dict, mozc rebuilt) OR a future
/// segmentation-pipeline pass that disambiguates from context.  Either
/// way, NOT this parser.
///
/// What survives:
///   - All-single-reading words ("快手" → kuai+shou=kuaishou, both
///     single-reading) → emitted with confidence.
///   - Words already in our library at a different code keep being
///     SKIP-library-wins (库自主) via STAGE 6 — that's the existing row,
///     not a new derivation.
///
/// What drops (counted into `ParseResult.dropped`, NEVER silent):
///   - Any char uncovered by reverse-index (non-Han, ASCII, PUA).
///   - Any char with > 1 reading (多音字 ambiguity).
///   - Empty word, bad freq.
fn parse_jieba_phrase(raw: &str, engine: EngineKind) -> Result<ParseResult, String> {
    if engine != EngineKind::Pinyin {
        return Err(format!(
            "jieba_phrase parser is pinyin-only (engine={})",
            engine.name(),
        ));
    }
    let mut rows = Vec::with_capacity(64_000);
    let mut dropped = 0u64;
    let mut dropped_ambiguous = 0u64;
    let mut dropped_uncovered = 0u64;
    let mut sample_drops: Vec<String> = Vec::with_capacity(8);

    for raw_line in raw.lines() {
        let t = raw_line.trim();
        if t.is_empty() || t.starts_with('#') { continue; }
        let mut it = t.split_whitespace();
        let (Some(word), Some(freq_s)) = (it.next(), it.next()) else { continue };
        let Ok(freq) = freq_s.parse::<u32>() else {
            dropped += 1;
            if sample_drops.len() < 5 { sample_drops.push(format!("{word} (bad freq {freq_s:?})")); }
            continue;
        };
        if word.is_empty() {
            dropped += 1;
            continue;
        }
        // Derive code via per-char SAFE reading: require exactly one
        // reading per char.  Multi-reading or uncovered → bail the row.
        let mut code = String::with_capacity(word.len() * 4);
        let mut bail_reason: Option<&'static str> = None;
        for ch in word.chars() {
            let readings = inputx_pinyin::char_to_pinyin(ch);
            if readings.is_empty() {
                bail_reason = Some("uncovered char");
                break;
            }
            if readings.len() > 1 {
                bail_reason = Some("ambiguous reading");
                break;
            }
            code.push_str(&readings[0]);
        }
        match bail_reason {
            None if !code.is_empty() => {
                rows.push((code, word.to_string(), None, freq));
            }
            Some("ambiguous reading") => {
                dropped += 1;
                dropped_ambiguous += 1;
                if sample_drops.len() < 5 { sample_drops.push(format!("{word} (多音字)")); }
            }
            Some("uncovered char") => {
                dropped += 1;
                dropped_uncovered += 1;
                if sample_drops.len() < 5 { sample_drops.push(format!("{word} (uncovered char)")); }
            }
            _ => {
                dropped += 1;
            }
        }
    }

    if dropped > 0 {
        eprintln!("            dropped {dropped} rows ({} 多音字 + {} 非 Han, sample: {})",
            dropped_ambiguous, dropped_uncovered, sample_drops.join(", "));
    }
    Ok(ParseResult { rows, dropped })
}

// ─── Fetch (fetch_kind dispatch) ────────────────────────────────────

struct FetchResult {
    bytes: Vec<u8>,
    sha256_hex: String,
}

fn fetch_file(url_or_path: &str) -> Result<FetchResult, String> {
    // For fetch_kind="file" the `fetch_url` is a path relative to repo
    // root (or absolute).  Keeps the registry portable across checkouts.
    let p = if Path::new(url_or_path).is_absolute() {
        PathBuf::from(url_or_path)
    } else {
        repo_root().join(url_or_path)
    };
    let bytes = fs::read(&p).map_err(|e| format!("fetch_file {}: {e}", p.display()))?;
    let mut h = Sha256::new();
    h.update(&bytes);
    let sha256_hex = hex_encode(&h.finalize());
    Ok(FetchResult { bytes, sha256_hex })
}

/// HTTPS fetch for `fetch_kind="http"`.  Writes a side-effect cache
/// under `tools/scoring/data/cache/corpus_digest/<source_id>.bin` so a
/// failed network later can still re-ingest from the last-known-good
/// payload (Phase B-5 `--offline` flag will surface this).  Cache is
/// gitignored.
///
/// NO HEAD-request short-circuit yet — every call does a full GET.  An
/// ETag-driven `corpus-digest check` lives in Phase B-5.  STAGE 1's
/// "sha unchanged → no-op" logic in `cmd_ingest` already short-circuits
/// the heavy parse/diff stages once the bytes are in, so this is
/// expensive only by bandwidth, not by CPU.
fn fetch_http(url: &str, source_id: &str) -> Result<FetchResult, String> {
    eprintln!("            HTTP GET {url}");
    let resp = ureq::get(url).call()
        .map_err(|e| format!("HTTP GET {url}: {e}"))?;
    let status = resp.status();
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {url} returned status {status}"));
    }

    let mut bytes = Vec::new();
    resp.into_reader()
        .take(512 * 1024 * 1024)  // 512 MiB hard cap; jieba dict.txt is ~5 MiB
        .read_to_end(&mut bytes)
        .map_err(|e| format!("read body: {e}"))?;

    let mut h = Sha256::new();
    h.update(&bytes);
    let sha256_hex = hex_encode(&h.finalize());

    // Side-effect cache.  Failures are non-fatal — we still return the
    // bytes; the next ingest just won't have a cached fallback.
    let dir = cache_dir();
    if let Err(e) = fs::create_dir_all(&dir) {
        eprintln!("            (cache create_dir {} failed: {e}; continuing without cache)", dir.display());
    } else {
        let bin = dir.join(format!("{source_id}.bin"));
        let sha = dir.join(format!("{source_id}.sha256"));
        if let Err(e) = fs::write(&bin, &bytes) {
            eprintln!("            (cache write {} failed: {e})", bin.display());
        }
        if let Err(e) = fs::write(&sha, &sha256_hex) {
            eprintln!("            (cache write {} failed: {e})", sha.display());
        }
    }

    Ok(FetchResult { bytes, sha256_hex })
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn sha256_file(p: &Path) -> Result<String, String> {
    let bytes = fs::read(p).map_err(|e| format!("sha256 {}: {e}", p.display()))?;
    let mut h = Sha256::new();
    h.update(&bytes);
    Ok(hex_encode(&h.finalize()))
}

// ─── Library write (STAGE 7) ────────────────────────────────────────

/// Engine-aware row sort for byte-stable output.
///
/// Per PLAN-corpus-digest §6:
///   - pinyin / nihongo: byte-lex by (code, word)
///   - wubi             : by (layer asc, code asc, word asc)
///
/// We add tie-breakers (extra, source) so equal-key rows have a
/// deterministic relative order regardless of input order.
///
/// CALIBRATION (2026-06-03):
///   - pinyin: actual library.tsv is byte-sorted by (code, word); my
///     key matches → no-op rewrite byte-identical (verified before
///     Phase B-2 land).
///   - wubi  : actual library.tsv is byte-sorted by (code, word, layer)
///     — DIFFERENT from PLAN's (layer, code, word). PLAN-corpus-digest
///     §6 is the spec; first real wubi ingest will renormalize. Until
///     then, dry-run only.
///   - nihongo: actual library.tsv preserves codegen insertion order
///     (jukugo block, then kanji block, no internal byte sort). First
///     real nihongo ingest will renormalize to PLAN's (code, word).
fn sort_rows(rows: &mut [LibraryRow], engine: EngineKind) {
    match engine {
        EngineKind::Pinyin | EngineKind::Nihongo => rows.sort_by(|a, b| {
            a.code.cmp(&b.code)
                .then_with(|| a.word.cmp(&b.word))
                .then_with(|| a.extra.cmp(&b.extra))
                .then_with(|| a.source.cmp(&b.source))
        }),
        EngineKind::Wubi => rows.sort_by(|a, b| {
            a.extra.cmp(&b.extra)
                .then_with(|| a.code.cmp(&b.code))
                .then_with(|| a.word.cmp(&b.word))
                .then_with(|| a.source.cmp(&b.source))
        }),
    }
}

/// Atomic write: tempfile in same dir + fsync + rename.
fn write_library_atomic(engine: EngineKind, header: &str, rows: &[LibraryRow]) -> Result<(), String> {
    let target = library_path(engine.name());
    let dir = target.parent().ok_or_else(|| format!("no parent dir for {}", target.display()))?;
    let tmp = dir.join(format!(".library.tsv.tmp.{}", std::process::id()));

    {
        let mut f = fs::File::create(&tmp)
            .map_err(|e| format!("create {}: {e}", tmp.display()))?;
        f.write_all(header.as_bytes()).map_err(|e| format!("write header: {e}"))?;
        for r in rows {
            match (&r.extra, engine) {
                (None, EngineKind::Pinyin) => {
                    writeln!(f, "{}\t{}\t{}\t{}", r.code, r.word, r.freq, r.source)
                        .map_err(|e| format!("write row: {e}"))?;
                }
                (Some(extra), EngineKind::Wubi | EngineKind::Nihongo) => {
                    writeln!(f, "{}\t{}\t{}\t{}\t{}", r.code, r.word, extra, r.freq, r.source)
                        .map_err(|e| format!("write row: {e}"))?;
                }
                _ => return Err(format!(
                    "row shape ↔ engine mismatch: extra={:?}, engine={}",
                    r.extra, engine.name(),
                )),
            }
        }
        f.sync_all().map_err(|e| format!("fsync {}: {e}", tmp.display()))?;
    }
    fs::rename(&tmp, &target)
        .map_err(|e| format!("rename {} → {}: {e}", tmp.display(), target.display()))?;
    Ok(())
}

// ─── Read-only commands ─────────────────────────────────────────────

fn cmd_list() -> Result<(), String> {
    let reg = load_registry()?;
    println!("source_registry version={}  ({} sources)\n", reg.version, reg.sources.len());
    println!("{:<22} {:<8} {:<10} {:<10} {:>10}  last_ingested",
        "source_id", "engine", "type", "status", "rows");
    println!("{}", "─".repeat(86));
    for s in &reg.sources {
        println!("{:<22} {:<8} {:<10} {:<10} {:>10}  {}",
            s.source_id, s.engine, s.source_type, s.status,
            s.current_row_count, s.last_ingested_at);
    }
    Ok(())
}

fn cmd_show(source_id: &str) -> Result<(), String> {
    let reg = load_registry()?;
    let Some(s) = reg.sources.iter().find(|s| s.source_id == source_id) else {
        return Err(format!("source_id {source_id:?} not found in registry"));
    };
    println!("source_id           = {}", s.source_id);
    println!("engine              = {}", s.engine);
    println!("source_type         = {}", s.source_type);
    println!("source_name         = {}", s.source_name);
    println!("fetch_kind          = {}", s.fetch_kind);
    println!("fetch_url           = {}", s.fetch_url);
    println!("current_version     = {}", s.current_version);
    println!("current_sha256      = {}", s.current_sha256);
    println!("last_event_id       = {}", s.last_event_id);
    println!("last_ingested_at    = {}", s.last_ingested_at);
    println!("current_row_count   = {}", s.current_row_count);
    println!("ingest_format       = {}", s.ingest_format);
    println!("auto_update         = {}", s.auto_update);
    println!("auto_fetch_schedule = {}", s.auto_fetch_schedule);
    println!("status              = {}", s.status);
    println!("notes:");
    for line in s.notes.lines() {
        println!("  {line}");
    }
    Ok(())
}

fn cmd_events(engine_filter: Option<&str>) -> Result<(), String> {
    let log = load_log()?;
    println!("digest_log version={}  ({} events)\n", log.version, log.events.len());
    println!("{:<32} {:<8} {:<10} {:>10} {:>10} {:>10}  ingested",
        "event_id", "engine", "type", "added", "updated", "rejected");
    println!("{}", "─".repeat(100));
    for e in &log.events {
        if let Some(f) = engine_filter {
            if e.engine != f { continue; }
        }
        println!("{:<32} {:<8} {:<10} {:>10} {:>10} {:>10}  {}",
            e.event_id, e.engine, e.source_type,
            e.rows_added, e.rows_updated, e.rows_rejected, e.ingested_at);
    }
    Ok(())
}

// ─── Ingest (STAGE 1–9) ─────────────────────────────────────────────

struct IngestPlan {
    /// (code, word, extra, freq_after_fold)
    added: Vec<(String, String, Option<String>, u32)>,
    skipped_same:          u64,
    /// PLAN §5 v2: 库自主 — upstream freq ≠ my freq → 我消化过,我的对.
    skipped_library_wins:  u64,
    skipped_polish:        u64,
    rejected_garbage:      u64,
}

/// STAGE 3.5: rank-fold normalization parameters.
///
/// log-log linear regression over (upstream_freq, my_freq) consensus
/// pairs, with p5/p95 clamp on my_freqs to prevent extrapolation
/// blowing past the in-distribution range.
///
/// Used to translate upstream `freq` values into our library's freq
/// scale when an ADD row needs a freq.  See PLAN-corpus-digest §5.1.
#[derive(Debug)]
struct FreqFold {
    a: f64,
    b: f64,
    p5: u32,
    p95: u32,
    n_pairs: usize,
}

/// Refuse to ingest when fewer than this many consensus pairs exist.
/// Below 64 the log-log regression has high variance and the fold
/// function is unreliable — better to error than fold blindly.
const MIN_FOLD_PAIRS: usize = 64;

impl FreqFold {
    /// Translate one upstream freq into our library scale.
    /// Upstream 0 → p5 (floor); else `exp(a + b·ln(j))` clamped to
    /// [p5, p95] so we never assign a brand-new word a freq above
    /// what the consensus distribution allows.
    fn apply(&self, upstream_freq: u32) -> u32 {
        if upstream_freq == 0 { return self.p5; }
        let log_j = (upstream_freq as f64).ln();
        let log_m = self.a + self.b * log_j;
        let m = log_m.exp();
        m.max(self.p5 as f64).min(self.p95 as f64).round() as u32
    }
}

/// Build STAGE 3.5 fold from the upstream tuples + our library.
///
/// Excludes polish rows from the regression: polish freqs are hand-set
/// (e.g. 500 for "寄了") and don't follow corpus statistics — they'd
/// pollute the fit.
///
/// Pairs must have both freqs > 0 (log(0) is undefined; treat zero
/// freqs as "no signal" rather than fitting them).
fn build_freq_fold(library: &[LibraryRow], upstream: &[IngestTuple]) -> Result<FreqFold, String> {
    use std::collections::HashMap;
    let mut my_index: HashMap<(String, String), u32> = HashMap::with_capacity(library.len());
    for r in library {
        if r.source == "digested" && r.freq > 0 {
            my_index.insert((r.code.clone(), r.word.clone()), r.freq);
        }
    }

    let mut pairs: Vec<(u32, u32)> = Vec::new();
    for (c, w, _, j) in upstream {
        if *j == 0 { continue; }
        if let Some(&m) = my_index.get(&(c.clone(), w.clone())) {
            pairs.push((*j, m));
        }
    }

    if pairs.len() < MIN_FOLD_PAIRS {
        return Err(format!(
            "STAGE 3.5: only {} consensus pairs (need ≥ {}). Refusing to ingest blind — \
             this source's freq scale can't be calibrated against our library.",
            pairs.len(), MIN_FOLD_PAIRS,
        ));
    }

    // log-log linear regression.
    let n = pairs.len() as f64;
    let log_js: Vec<f64> = pairs.iter().map(|(j, _)| (*j as f64).ln()).collect();
    let log_ms: Vec<f64> = pairs.iter().map(|(_, m)| (*m as f64).ln()).collect();
    let mean_j: f64 = log_js.iter().sum::<f64>() / n;
    let mean_m: f64 = log_ms.iter().sum::<f64>() / n;
    let cov: f64 = log_js.iter().zip(&log_ms)
        .map(|(j, m)| (j - mean_j) * (m - mean_m))
        .sum::<f64>() / n;
    let var: f64 = log_js.iter()
        .map(|j| (j - mean_j).powi(2))
        .sum::<f64>() / n;
    if var < 1e-9 {
        return Err(format!(
            "STAGE 3.5: upstream freq variance ≈ 0 across {} consensus pairs — fold undefined.",
            pairs.len(),
        ));
    }
    let b = cov / var;
    let a = mean_m - b * mean_j;

    // Percentiles of my_freqs (clamp range).
    let mut my_freqs: Vec<u32> = pairs.iter().map(|(_, m)| *m).collect();
    my_freqs.sort_unstable();
    let p5_idx = ((pairs.len() as f64) * 0.05).floor() as usize;
    let p95_idx = (((pairs.len() as f64) * 0.95).floor() as usize).min(pairs.len() - 1);

    Ok(FreqFold {
        a, b,
        p5: my_freqs[p5_idx],
        p95: my_freqs[p95_idx],
        n_pairs: pairs.len(),
    })
}

fn cmd_ingest(source_id: &str, apply: bool, rationale: Option<&str>, today: &str) -> Result<(), String> {
    let reg = load_registry()?;
    let src = reg.sources.iter().find(|s| s.source_id == source_id)
        .ok_or_else(|| format!("source_id {source_id:?} not found in registry"))?
        .clone();

    eprintln!("[ingest] source_id     = {}", src.source_id);
    eprintln!("[ingest] engine        = {}", src.engine);
    eprintln!("[ingest] fetch_kind    = {}", src.fetch_kind);
    eprintln!("[ingest] ingest_format = {}", src.ingest_format);

    let engine = EngineKind::parse(&src.engine)?;
    match src.ingest_format.as_str() {
        "simple_tsv" | "jieba_phrase" => {}
        other => return Err(format!(
            "unknown ingest_format {other:?}. supported: simple_tsv, jieba_phrase \
             (unihan / mozc / cc_cedict land later).",
        )),
    }
    match src.fetch_kind.as_str() {
        "file" | "http" => {},
        "static" => {
            // legacy anchors — re-ingest is meaningless (no upstream to
            // diff against).  Block clearly rather than silently no-op.
            return Err(format!(
                "source {:?} is fetch_kind=static (legacy anchor). Anchors are bootstrap-only \
                 — no upstream artifact exists to re-ingest. Register a real fetch_kind=file/http \
                 source instead.",
                src.source_id
            ));
        },
        other => return Err(format!("Phase B-1: fetch_kind ∈ {{file, http}} (got {other:?})")),
    }

    // ── STAGE 1: fetch ──────────────────────────────────────────────
    eprintln!("[stage 1/9] fetch");
    let fetched = match src.fetch_kind.as_str() {
        "file" => fetch_file(&src.fetch_url)?,
        "http" => fetch_http(&src.fetch_url, &src.source_id)?,
        _ => unreachable!("validated above"),
    };
    eprintln!("            sha256 = {}", fetched.sha256_hex);
    eprintln!("            bytes  = {}", fetched.bytes.len());

    // I-3 早退路径:同 sha 重跑 → no-op,在解析前先短路。
    if src.current_sha256 == fetched.sha256_hex && !src.current_sha256.is_empty() && src.current_sha256 != "n/a" {
        eprintln!("[ingest] upstream sha256 unchanged from registry — full no-op. exit.");
        return Ok(());
    }

    let text = std::str::from_utf8(&fetched.bytes)
        .map_err(|e| format!("source content not utf-8: {e}"))?;

    // ── STAGE 2: parse ──────────────────────────────────────────────
    eprintln!("[stage 2/9] parse ({})", src.ingest_format);
    let parse_result = match src.ingest_format.as_str() {
        "simple_tsv"   => parse_simple_tsv(text, engine)?,
        "jieba_phrase" => parse_jieba_phrase(text, engine)?,
        _ => unreachable!("validated above"),
    };
    eprintln!("            parsed rows = {}", parse_result.rows.len());

    // ── STAGE 3: normalize (Phase B-3: trim only — NFC TBD Phase B-4) ──
    eprintln!("[stage 3/9] normalize (trim only — NFC deferred to Phase B-4)");
    let normalized: Vec<IngestTuple> = parse_result.rows.into_iter()
        .map(|(c, w, e, f)| (
            c.trim().to_string(),
            w.trim().to_string(),
            e.map(|x| x.trim().to_string()),
            f,
        ))
        .filter(|(c, w, _, _)| !c.is_empty() && !w.is_empty())
        .collect();

    // ── STAGE 4: dedupe in-source — (code, word) → max freq ─────────
    //
    // PLAN-corpus-digest §6 keys the digest_index on (code, word) — so
    // we do the same here. For wubi/nihongo the extra col is carried
    // along with the winning row but does not participate in the key.
    // If the same (code, word) appears twice with different extras,
    // max-freq wins; the extra of the winner is kept.
    eprintln!("[stage 4/9] dedupe in-source (max freq wins)");
    let mut deduped: BTreeMap<(String, String), (Option<String>, u32)> = BTreeMap::new();
    for (c, w, e, f) in normalized {
        let slot = deduped.entry((c, w)).or_insert((None, 0));
        if f > slot.1 { *slot = (e, f); }
    }
    eprintln!("            after dedupe = {}", deduped.len());

    // ── STAGE 5: garbage filter (D1 future-proof) ───────────────────
    eprintln!("[stage 5/9] garbage filter");
    let garbage = load_garbage_filter()?;
    let mut rejected = 0u64;
    deduped.retain(|(c, w), _| {
        if garbage.contains(&(c.clone(), w.clone())) {
            rejected += 1;
            false
        } else {
            true
        }
    });
    eprintln!("            rejected = {rejected} (parser dropped {} more in STAGE 2)",
        parse_result.dropped);
    // Total rows that didn't make it past STAGE 2-5 — recorded in the
    // event's `rows_rejected` for full audit.
    let total_rejected = rejected + parse_result.dropped;

    // ── STAGE 3.5: build freq fold ──────────────────────────────────
    //
    // PLAN-corpus-digest §5.1: ADD freqs go through a log-log
    // regression mapping `upstream_freq → my_freq`, fitted on consensus
    // (code, word) pairs.  Refuses ingest if < MIN_FOLD_PAIRS consensus
    // pairs — keeps us from blindly fitting noise into the library.
    //
    // Built before STAGE 6 so STAGE 6 ADD branch can apply it row-by-row.
    eprintln!("[stage 3.5/9] freq fold (log-log linear)");
    let lib = load_library(engine)?;
    let fold_input: Vec<IngestTuple> = deduped.iter()
        .map(|((c, w), (e, j))| (c.clone(), w.clone(), e.clone(), *j))
        .collect();
    let fold = build_freq_fold(&lib.rows, &fold_input)?;
    eprintln!("            n_pairs = {}", fold.n_pairs);
    eprintln!("            slope b = {:.4}  intercept a = {:.4}", fold.b, fold.a);
    eprintln!("            clamp = [p5={}, p95={}]", fold.p5, fold.p95);

    // ── STAGE 6: diff against library ───────────────────────────────
    eprintln!("[stage 6/9] diff against library");
    let mut lib = lib;
    let mut lib_index: BTreeMap<(String, String), usize> = BTreeMap::new();
    for (i, r) in lib.rows.iter().enumerate() {
        lib_index.insert((r.code.clone(), r.word.clone()), i);
    }
    let mut plan = IngestPlan {
        added: Vec::new(),
        skipped_same: 0,
        skipped_library_wins: 0,
        skipped_polish: 0,
        rejected_garbage: total_rejected,
    };
    // Track added-row freq distribution for the report + audit.
    let mut added_freqs: Vec<u32> = Vec::new();
    for ((code, word), (extra, upstream_freq)) in deduped {
        match lib_index.get(&(code.clone(), word.clone())) {
            None => {
                // I-1 / "吃" semantics: new word → fold upstream freq
                // into our scale, ADD with that freq.
                let folded = fold.apply(upstream_freq);
                added_freqs.push(folded);
                plan.added.push((code, word, extra, folded));
            }
            Some(&idx) => {
                let r = &lib.rows[idx];
                if r.source == "polish" {
                    plan.skipped_polish += 1;
                } else if r.freq == upstream_freq {
                    plan.skipped_same += 1;
                } else {
                    // PLAN §5 v2: 库自主 — upstream freq ≠ mine means
                    // I've digested it differently; my freq is correct.
                    plan.skipped_library_wins += 1;
                }
            }
        }
    }

    // Added-freq summary (audit lens).
    let (add_min, add_med, add_max) = if added_freqs.is_empty() {
        (0, 0, 0)
    } else {
        let mut s = added_freqs.clone(); s.sort_unstable();
        (s[0], s[s.len() / 2], s[s.len() - 1])
    };
    eprintln!("            ADD                       = {} (freq: min={}, med={}, max={})",
        plan.added.len(), add_min, add_med, add_max);
    eprintln!("            SKIP (freq unchanged)     = {}", plan.skipped_same);
    eprintln!("            SKIP (library wins, v2)   = {}", plan.skipped_library_wins);
    eprintln!("            SKIP (polish wins)        = {}", plan.skipped_polish);
    eprintln!("            REJECT (garbage / drops)  = {}", plan.rejected_garbage);

    if !apply {
        eprintln!("\n[dry-run] no changes written.  Re-run with --apply to commit.");
        return Ok(());
    }

    // ── Bail-out if nothing changes (I-3 final guard) ───────────────
    if plan.added.is_empty() {
        // STAGE 7+8+9 are no-ops; preserve append-only invariant by NOT
        // emitting a stub event.  v2 "吃" semantics: SKIP-library-wins
        // counts even when 311k rows differed — that's library autonomy
        // acting correctly, NOT a write event.
        eprintln!("\n[apply] no new words → no library write, no event written.");
        return Ok(());
    }

    // ── STAGE 7: write library.tsv (atomic) ─────────────────────────
    eprintln!("[stage 7/9] write library.tsv (atomic) — appending {} new digested rows",
        plan.added.len());
    for (code, word, extra, freq) in &plan.added {
        lib.rows.push(LibraryRow {
            code: code.clone(), word: word.clone(),
            extra: extra.clone(),
            freq: *freq, source: "digested".into(),
        });
    }
    sort_rows(&mut lib.rows, engine);
    write_library_atomic(engine, &lib.header, &lib.rows)?;
    let lib_sha_after = sha256_file(&library_path(engine.name()))?;
    eprintln!("            library_sha256_after = {lib_sha_after}");

    // ── STAGE 8: append digest_log event ────────────────────────────
    eprintln!("[stage 8/9] append digest_log event");
    let event_id = format!("{}-{}", src.source_id, today);
    // Stuff the fold params + counts into notes so future audits can
    // reproduce this ingest's freq translation choices.  PLAN-corpus-
    // digest §5.1 names these fields verbatim.
    let mut full_notes = String::new();
    if let Some(r) = rationale {
        full_notes.push_str(r);
        full_notes.push_str("\n\n");
    }
    full_notes.push_str(&format!(
        "fold_a = {:.4}\nfold_b = {:.4}\nfold_p5 = {}\nfold_p95 = {}\nfold_n_pairs = {}\n",
        fold.a, fold.b, fold.p5, fold.p95, fold.n_pairs,
    ));
    full_notes.push_str(&format!(
        "added_freq_min = {}\nadded_freq_med = {}\nadded_freq_max = {}\n",
        add_min, add_med, add_max,
    ));
    full_notes.push_str(&format!(
        "skipped_same = {}\nskipped_library_wins = {}\nskipped_polish = {}\n",
        plan.skipped_same, plan.skipped_library_wins, plan.skipped_polish,
    ));

    let event = Event {
        event_id: event_id.clone(),
        engine: src.engine.clone(),
        source_type: src.source_type.clone(),
        source_name: src.source_name.clone(),
        source_version: src.current_version.clone(),
        source_sha256: fetched.sha256_hex.clone(),
        ingested_at: today.into(),
        rows_added: plan.added.len() as u64,
        rows_updated: 0,  // v2: UPDATE branch retired
        rows_rejected: plan.rejected_garbage,
        library_sha256_after: lib_sha_after.clone(),
        notes: full_notes,
    };
    append_event(&event)?;

    // ── STAGE 9: update source_registry entry ───────────────────────
    eprintln!("[stage 9/9] update source_registry");
    let mut row_count_after = 0u64;
    // Recompute approximate row count for this source: we don't keep a
    // per-row source attribution, so for a self/external source we
    // store "digested row count touched by this event id" as a proxy.
    // Phase A simplification: current_row_count = lib's digested-rows
    // total (whole library minus polish). It's "approximate" by design
    // (per PLAN §1.3 "current_row_count: approximate").
    for r in &lib.rows {
        if r.source == "digested" { row_count_after += 1; }
    }
    update_registry_source(&src.source_id, |s| {
        s.current_sha256 = fetched.sha256_hex.clone();
        s.last_event_id = event_id.clone();
        s.last_ingested_at = today.into();
        s.current_row_count = row_count_after;
    })?;

    eprintln!("\n[ingest] ✓ event {event_id} written. library now {row_count_after} digested rows + N polish rows.");
    Ok(())
}

/// Append-only write to digest_log.toml. We don't round-trip through
/// serde (toml's serializer reorders, kills hand-edited comments).
/// Instead: read raw, append a textual `[[event]]` block at EOF, fsync.
fn append_event(ev: &Event) -> Result<(), String> {
    let path = digest_log_path();
    let mut block = String::new();
    block.push_str("\n# ─── auto-emitted by corpus-digest ────────────────────────────────\n");
    block.push_str("[[event]]\n");
    block.push_str(&format!("event_id = {}\n", toml_str(&ev.event_id)));
    block.push_str(&format!("engine = {}\n", toml_str(&ev.engine)));
    block.push_str(&format!("source_type = {}\n", toml_str(&ev.source_type)));
    block.push_str(&format!("source_name = {}\n", toml_str(&ev.source_name)));
    block.push_str(&format!("source_version = {}\n", toml_str(&ev.source_version)));
    block.push_str(&format!("source_sha256 = {}\n", toml_str(&ev.source_sha256)));
    block.push_str(&format!("ingested_at = {}\n", toml_str(&ev.ingested_at)));
    block.push_str(&format!("rows_added = {}\n", ev.rows_added));
    block.push_str(&format!("rows_updated = {}\n", ev.rows_updated));
    block.push_str(&format!("rows_rejected = {}\n", ev.rows_rejected));
    block.push_str(&format!("library_sha256_after = {}\n", toml_str(&ev.library_sha256_after)));
    if ev.notes.is_empty() {
        block.push_str("notes = \"\"\n");
    } else {
        block.push_str(&format!("notes = {}\n", toml_multiline(&ev.notes)));
    }

    // Round-trip parse the existing file to make sure we didn't corrupt
    // it on a prior crash before appending.
    let existing = fs::read_to_string(&path)
        .map_err(|e| format!("read {}: {e}", path.display()))?;
    let _: DigestLog = toml::from_str(&existing)
        .map_err(|e| format!("digest_log {} is malformed before append; refusing to append: {e}", path.display()))?;

    let tmp = path.with_extension("toml.tmp");
    {
        let mut f = fs::File::create(&tmp)
            .map_err(|e| format!("create {}: {e}", tmp.display()))?;
        f.write_all(existing.as_bytes()).map_err(|e| format!("write existing: {e}"))?;
        if !existing.ends_with('\n') {
            f.write_all(b"\n").map_err(|e| format!("write nl: {e}"))?;
        }
        f.write_all(block.as_bytes()).map_err(|e| format!("write block: {e}"))?;
        f.sync_all().map_err(|e| format!("fsync {}: {e}", tmp.display()))?;
    }
    fs::rename(&tmp, &path)
        .map_err(|e| format!("rename {} → {}: {e}", tmp.display(), path.display()))?;
    Ok(())
}

/// Re-serialize the registry with one source updated. The registry IS
/// rewriteable (it's "current state", not history) so a round-trip
/// through serde+toml is acceptable.  The trade-off: hand-edited
/// comments get stripped on the first machine write.  For Phase A we
/// accept this; Phase B can switch to a comment-preserving edit if it
/// becomes a problem.
fn update_registry_source<F: FnMut(&mut Source)>(source_id: &str, mut mutate: F) -> Result<(), String> {
    let path = registry_path();
    let mut reg = load_registry()?;
    let Some(s) = reg.sources.iter_mut().find(|s| s.source_id == source_id) else {
        return Err(format!("update_registry: source_id {source_id:?} not found"));
    };
    mutate(s);

    // Serialize.  We prepend the original header comments so the file
    // remains documented; only the `[[source]]` blocks get rewritten.
    let raw = fs::read_to_string(&path)
        .map_err(|e| format!("read {}: {e}", path.display()))?;
    let header: String = raw.lines()
        .take_while(|l| {
            let t = l.trim_start();
            t.is_empty() || t.starts_with('#')
        })
        .map(|l| { let mut s = l.to_string(); s.push('\n'); s })
        .collect();

    let mut body = String::new();
    body.push_str(&format!("\nversion = {}\n", reg.version));
    for s in &reg.sources {
        body.push_str("\n[[source]]\n");
        body.push_str(&format!("source_id = {}\n", toml_str(&s.source_id)));
        body.push_str(&format!("engine = {}\n", toml_str(&s.engine)));
        body.push_str(&format!("source_type = {}\n", toml_str(&s.source_type)));
        body.push_str(&format!("source_name = {}\n", toml_str(&s.source_name)));
        body.push_str(&format!("fetch_kind = {}\n", toml_str(&s.fetch_kind)));
        body.push_str(&format!("fetch_url = {}\n", toml_str(&s.fetch_url)));
        body.push_str(&format!("current_version = {}\n", toml_str(&s.current_version)));
        body.push_str(&format!("current_sha256 = {}\n", toml_str(&s.current_sha256)));
        body.push_str(&format!("last_event_id = {}\n", toml_str(&s.last_event_id)));
        body.push_str(&format!("last_ingested_at = {}\n", toml_str(&s.last_ingested_at)));
        body.push_str(&format!("current_row_count = {}\n", s.current_row_count));
        body.push_str(&format!("ingest_format = {}\n", toml_str(&s.ingest_format)));
        body.push_str(&format!("auto_update = {}\n", s.auto_update));
        body.push_str(&format!("auto_fetch_schedule = {}\n", toml_str(&s.auto_fetch_schedule)));
        body.push_str(&format!("status = {}\n", toml_str(&s.status)));
        if s.notes.is_empty() {
            body.push_str("notes = \"\"\n");
        } else {
            body.push_str(&format!("notes = {}\n", toml_multiline(&s.notes)));
        }
    }

    let tmp = path.with_extension("toml.tmp");
    {
        let mut f = fs::File::create(&tmp)
            .map_err(|e| format!("create {}: {e}", tmp.display()))?;
        f.write_all(header.as_bytes()).map_err(|e| format!("write header: {e}"))?;
        f.write_all(body.as_bytes()).map_err(|e| format!("write body: {e}"))?;
        f.sync_all().map_err(|e| format!("fsync {}: {e}", tmp.display()))?;
    }
    fs::rename(&tmp, &path)
        .map_err(|e| format!("rename {} → {}: {e}", tmp.display(), path.display()))?;

    // Round-trip verify: make sure the file we just wrote round-trips
    // back to the same SourceRegistry shape.
    let _: SourceRegistry = toml::from_str(
        &fs::read_to_string(&path).map_err(|e| format!("re-read {}: {e}", path.display()))?,
    ).map_err(|e| format!("registry I wrote doesn't parse: {e}"))?;
    Ok(())
}

/// Quote `s` as a TOML basic string. Escapes `\`, `"`, control chars.
fn toml_str(s: &str) -> String {
    // If multi-line, use triple-quoted form instead.
    if s.contains('\n') { return toml_multiline(s); }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"'  => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Triple-quoted TOML multi-line string. We don't escape inside (TOML
/// allows literal newlines + most chars), but we guard against `"""`
/// sequences.
fn toml_multiline(s: &str) -> String {
    if s.contains("\"\"\"") {
        // Fallback to basic string with explicit \n escapes.
        let escaped = s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
        return format!("\"{escaped}\"");
    }
    let mut out = String::with_capacity(s.len() + 8);
    out.push_str("\"\"\"\n");
    out.push_str(s);
    if !s.ends_with('\n') { out.push('\n'); }
    out.push_str("\"\"\"");
    out
}

// ─── CLI ────────────────────────────────────────────────────────────

const USAGE: &str = "\
corpus-digest — 3-layer corpus pipeline tool (Phase A MVP)

USAGE:
  corpus-digest list
  corpus-digest show <source_id>
  corpus-digest events [--engine pinyin|wubi|nihongo]
  corpus-digest ingest <source_id> [--apply] [--rationale <text>] [--today YYYY-MM-DD]

DESIGN: .claude/PLAN-corpus-digest.md

INVARIANTS protected by ingest:
  I-1 polish > digested, always (polish rows never touched)
  I-2 library 自主性 (upstream removals never DELETE)
  I-3 idempotent (same sha256 re-ingest = no-op)
  I-4 三层原子 (library + log + registry coordinated writes)
  I-5 digest_log append-only; registry rewriteable as 'current state'
  I-6 library rows source-agnostic (digested|polish only)
";

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().collect();
    if argv.len() < 2 {
        eprint!("{USAGE}");
        return ExitCode::from(2);
    }
    let result = match argv[1].as_str() {
        "list" => cmd_list(),
        "show" => {
            if argv.len() != 3 {
                eprintln!("usage: corpus-digest show <source_id>");
                return ExitCode::from(2);
            }
            cmd_show(&argv[2])
        }
        "events" => {
            let mut engine: Option<&str> = None;
            let mut i = 2;
            while i < argv.len() {
                if argv[i] == "--engine" {
                    if i + 1 >= argv.len() {
                        eprintln!("--engine needs a value"); return ExitCode::from(2);
                    }
                    engine = Some(&argv[i+1]); i += 2;
                } else {
                    eprintln!("unknown arg: {}", argv[i]); return ExitCode::from(2);
                }
            }
            cmd_events(engine)
        }
        "ingest" => {
            if argv.len() < 3 {
                eprintln!("usage: corpus-digest ingest <source_id> [--apply] [--rationale <text>] [--today YYYY-MM-DD]");
                return ExitCode::from(2);
            }
            let source_id = argv[2].clone();
            let mut apply = false;
            let mut rationale: Option<String> = None;
            let mut today: Option<String> = None;
            let mut i = 3;
            while i < argv.len() {
                match argv[i].as_str() {
                    "--apply" => { apply = true; i += 1; }
                    "--rationale" => {
                        if i + 1 >= argv.len() {
                            eprintln!("--rationale needs a value"); return ExitCode::from(2);
                        }
                        rationale = Some(argv[i+1].clone()); i += 2;
                    }
                    "--today" => {
                        if i + 1 >= argv.len() {
                            eprintln!("--today needs a value"); return ExitCode::from(2);
                        }
                        today = Some(argv[i+1].clone()); i += 2;
                    }
                    other => {
                        eprintln!("unknown arg: {other}"); return ExitCode::from(2);
                    }
                }
            }
            // No Date::now() in Rust std — caller must pass --today, OR
            // we shell out to `date +%Y-%m-%d`. The latter keeps the tool
            // self-contained while staying deterministic-with-input.
            let today = today.unwrap_or_else(today_fallback);
            cmd_ingest(&source_id, apply, rationale.as_deref(), &today)
        }
        "--help" | "-h" | "help" => {
            print!("{USAGE}");
            Ok(())
        }
        other => {
            eprintln!("unknown command: {other}\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::FAILURE
        }
    }
}

/// Best-effort current date in YYYY-MM-DD (UTC).  Used only when the
/// caller doesn't pass `--today`.  We don't take a `chrono`/`time` dep
/// just for this — shell out to `date -u +%Y-%m-%d`.
fn today_fallback() -> String {
    let out = std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%d"])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        }
        _ => "UNKNOWN-DATE".to_string(),
    }
}
