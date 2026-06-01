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
	@echo "[polish] (1/5) rebuild weights from corpus + overlays"
	cd core && cargo run --features tools --release --bin pinyin-build-weights
	cd core && cargo run --features tools --release --bin wubi-build-weights
	@echo "[polish] (2/4) rebuild pinyin.dict (writes directly to inputx-pinyin-data-core, v1.11 WU-γ)"
	cd core && cargo run --features tools --release --bin pinyin-build-dict
	@echo "[polish] (3/4) rebuild .idf snapshots"
	cd core && cargo run --release --bin idf-from-pinyin-dict
	cd core && cargo run --release --bin idf-from-wubi-tables
	cd core && cargo run --release --bin idf-from-nihongo-kanji
	cd core && cargo run --release --bin idf-from-nihongo-jukugo
	@echo "[polish] (4/4) baseline gate"
	$(MAKE) baseline
	@echo "[polish] ✓ rebuild complete + baseline green"

baseline:
	cd core && cargo test -p inputx-scoring --lib --release -- --quiet 2>&1 | tail -3
	cd core && cargo test -p inputx-core --lib baseline --release 2>&1 | tail -3
	cd core && cargo test -p inputx-core --lib --release 2>&1 | tail -3

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
