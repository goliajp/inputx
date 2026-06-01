# Inputx polish workflow — single-command rebuild targets.
#
# Per the v1.10/v1.11 design (docs/POLISH-ARCHITECTURE.md), all polish
# actions reduce to: edit a data asset (engine_weights.toml / overlay
# TSV / corpus manifest) + `make polish-rebuild` + baseline gate.
# This file collects those rebuild steps so users don't reassemble the
# multi-step chain from memory every time.
#
# Targets:
#   make polish-rebuild   — regenerate everything that depends on
#                           weights.tsv / engine_weights.toml /
#                           pinyin.dict / .idf files; sync data-core
#                           copies; run baseline gate.
#   make baseline         — run the 24-test baseline + the 288-test
#                           inputx-core lib suite. Used by CI + the
#                           tail of polish-rebuild.
#   make verify-byte-id   — verify the shipped .idf files match what
#                           the build chain produces from current
#                           source. Used to detect "did someone forget
#                           to commit a rebuilt .idf?" pre-push.

.PHONY: polish-rebuild baseline verify-byte-id help

help:
	@echo "Inputx polish workflow targets:"
	@echo "  make polish-rebuild   regenerate weights → dict → .idf, sync, baseline"
	@echo "  make baseline         run baseline tests (no rebuild)"
	@echo "  make verify-byte-id   confirm shipped .idf == rebuilt .idf"

# Full polish-rebuild chain. Steps in order:
#   1. Regenerate weights.tsv from corpus + overlays (pinyin + wubi).
#   2. Rebuild pinyin.dict from new weights.tsv (with quickfix /
#      modern-vocab overlays applied via MAX semantics).
#   3. Sync `inputx-pinyin/data/pinyin.dict` to
#      `inputx-pinyin-data-core/data/pinyin.dict` so PinyinDict::embedded()
#      reads the new bytes.
#   4. Rebuild every .idf snapshot from the new dict.
#   5. Run baseline tests as a gate.
polish-rebuild:
	@echo "[polish] (1/3) rebuild pinyin.dict from committed weights.tsv + overlays"
	@# DO NOT rebuild weights.tsv here. v1.11 reproducibility audit
	@# (2026-06-01) found `pinyin-build-weights` is non-deterministic
	@# (re-running with no manifest changes produces ~270k row diff
	@# vs committed). Until that's fixed, the committed weights.tsv
	@# IS the source of truth for polish — only overlays + dict +
	@# .idf get rebuilt. Use `make rebuild-weights` explicitly when
	@# corpus manifest changes.
	cd core && cargo run --features tools --release --bin pinyin-build-dict
	@echo "[polish] (2/3) rebuild .idf snapshots"
	cd core && cargo run --release --bin idf-from-pinyin-dict
	cd core && cargo run --release --bin idf-from-wubi-tables
	cd core && cargo run --release --bin idf-from-nihongo-kanji
	cd core && cargo run --release --bin idf-from-nihongo-jukugo
	@echo "[polish] (3/3) baseline gate"
	$(MAKE) baseline
	@echo "[polish] ✓ rebuild complete + baseline green"

# Explicit weights rebuild — corpus manifest changed, or audit run.
# Expect a large weights.tsv diff vs committed (build_weights is
# non-deterministic across runs); review before committing.
rebuild-weights:
	@echo "[rebuild-weights] regenerating from corpus + manifest"
	cd core && cargo run --features tools --release --bin pinyin-build-weights
	cd core && cargo run --features tools --release --bin wubi-build-weights
	@echo "[rebuild-weights] WARN: weights.tsv likely differs from committed; review diff"
	@echo "[rebuild-weights] then run: make polish-rebuild"

baseline:
	cd core && cargo test -p inputx-scoring --lib --release 2>&1 | tail -3
	cd core && cargo test -p inputx-core --lib baseline --release 2>&1 | tail -3 | tee /tmp/baseline-out
	@grep -q "test result: ok" /tmp/baseline-out || (echo "[baseline] FAIL — see output above" && exit 1)
	cd core && cargo test -p inputx-core --lib --release 2>&1 | tail -3 | tee /tmp/lib-out
	@grep -q "test result: ok" /tmp/lib-out || (echo "[baseline] lib FAIL — see output above" && exit 1)

# Re-run the build chain and compare the resulting .idf SHAs against
# the committed bytes. Detects "I forgot to commit the regenerated
# .idf" before the change reaches origin.
verify-byte-id:
	@echo "[verify] pre-rebuild SHAs:"
	@shasum -a 256 core/crates/inputx-pinyin-helpers/data/words.idf \
	                 core/crates/inputx-wubi-data/data/words.idf \
	                 core/crates/inputx-nihongo-data-kanji/data/kanji.idf \
	                 core/crates/inputx-nihongo-data-jukugo/data/jukugo.idf
	@echo "[verify] rebuilding ..."
	@$(MAKE) -s polish-rebuild
	@echo "[verify] post-rebuild SHAs:"
	@shasum -a 256 core/crates/inputx-pinyin-helpers/data/words.idf \
	                 core/crates/inputx-wubi-data/data/words.idf \
	                 core/crates/inputx-nihongo-data-kanji/data/kanji.idf \
	                 core/crates/inputx-nihongo-data-jukugo/data/jukugo.idf
	@echo "[verify] compare manually if SHAs differ above"
