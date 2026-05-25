# Inputx scoring-pipeline driver. See README.md for full step docs.
#
# Targets:
#   make rebuild    — full pipeline 01→08
#   make validate   — quality gates against current data/merged/
#   make clean      — wipe intermediate data/ (preserve raw + manifest)
#   make distclean  — wipe everything in data/
#   make diff-vs-shipped — top-100 diffs vs current shipped FST
#
# Steps are tab-indented per Make convention. Each step depends on
# the previous step's marker file under data/.markers/ so partial
# runs resume correctly.

DATA := data
MARK := $(DATA)/.markers
PYTHON := python3
ROOT := $(shell git rev-parse --show-toplevel)

.PHONY: rebuild validate clean distclean diff-vs-shipped identity-check gate1 help

help:
	@echo "Targets:"
	@echo "  make rebuild           — full pipeline (slow)"
	@echo "  make validate          — quality gates only"
	@echo "  make clean             — intermediate-state wipe"
	@echo "  make distclean         — full wipe"
	@echo "  make diff-vs-shipped   — top-N diff report"
	@echo ""
	@echo "Per-step (advanced):"
	@echo "  make 01-fetch 02-extract 03-normalize 04-layer-assign"
	@echo "  make 05-merge 06-llm-annotate 07-validate 08-pack"

rebuild: 08-pack

01-fetch: $(MARK)/01.done
$(MARK)/01.done:
	@echo "[01] fetch corpora (per 01_fetch/manifest.toml)"
	$(PYTHON) 01_fetch/run_all.py
	@mkdir -p $(MARK) && touch $@

02-extract: $(MARK)/02.done
$(MARK)/02.done: $(MARK)/01.done
	@echo "[02] extract per-source (word,count) via build_weights dump (Rust counter)"
	$(PYTHON) 02_extract/run_all.py
	@touch $@

# CP3b: default normalize is per-source-log-count (SCORING.md §03 intent —
# per-source scale-free — but keeping count magnitude; chosen over literal
# rank-based log-rank, which collapsed coverage). See .claude/PLAN-dict-pipeline.md.
03-normalize: $(MARK)/03.done
$(MARK)/03.done: $(MARK)/02.done
	@echo "[03] CP3b: per-source-log-count normalize -> merged/"
	$(PYTHON) 03_normalize/run_all.py
	@touch $@

# CP2 identity gate (counting-chain regression): the sum-then-log mode must
# still reproduce the bare build_weights baseline bit-for-bit. Builds to a
# scratch path so it does not disturb the main per-source-log-count merged/.
identity-check:
	@$(PYTHON) 03_normalize/run_all.py --mode sum-then-log --out $(DATA)/baseline/sum-then-log.tsv >/dev/null
	@cmp $(DATA)/baseline/weights-bare.tsv $(DATA)/baseline/sum-then-log.tsv \
	    && echo "[identity] OK  sum-then-log mode == bare build_weights baseline" \
	    || { echo "[identity] DRIFT — counting chain no longer reproduces build_weights"; exit 1; }

# CP3a engine-level gate: build a dict from pipeline weights (no overlay) into
# baseline/, then run the input_corpus regression against it via probe (which
# loads it through INPUTX_PINYIN_DICT). Pass GATE1_ARGS=--baseline for a
# report-only run. Does NOT touch the shipped data/pinyin.dict.
gate1: $(DATA)/merged/weights.tsv
	cd $(ROOT)/core && cargo run -q --release --features tools --bin pinyin-build-dict -- \
	    --weights $(ROOT)/tools/scoring/$(DATA)/merged/weights.tsv \
	    --out $(ROOT)/tools/scoring/$(DATA)/baseline/pipeline.dict --no-overlay
	$(PYTHON) 07_validate/gate1_regression_corpus.py \
	    --dict $(DATA)/baseline/pipeline.dict $(GATE1_ARGS)

04-layer-assign: $(MARK)/04.done
$(MARK)/04.done: $(MARK)/03.done
	@echo "[04] assign engine layers"
	$(PYTHON) 04_layer_assign/run_all.py
	@touch $@

05-merge: $(MARK)/05.done
$(MARK)/05.done: $(MARK)/04.done
	@echo "[05] merge sources → unified weights.tsv"
	$(PYTHON) 05_merge/run.py --out $(DATA)/merged/weights.tsv
	@touch $@

06-llm-annotate: $(MARK)/06.done
$(MARK)/06.done: $(MARK)/05.done
	@echo "[06] LLM annotate ambiguous codes"
	$(PYTHON) 06_llm_annotate/run.py \
	    --in $(DATA)/merged/weights.tsv \
	    --out $(DATA)/annotations/llm_overrides.tsv
	@touch $@

07-validate: $(MARK)/07.done
$(MARK)/07.done: $(MARK)/06.done
	@echo "[07] quality gates"
	$(PYTHON) 07_validate/run_all.py
	@touch $@

08-pack: $(MARK)/08.done
$(MARK)/08.done: $(MARK)/07.done
	@echo "[08] pack into FST artifacts"
	$(PYTHON) 08_pack/run.py
	@touch $@

validate:
	$(PYTHON) 07_validate/run_all.py --against-shipped

clean:
	rm -rf $(DATA)/extracted $(DATA)/normalized $(DATA)/layers \
	       $(DATA)/merged $(DATA)/annotations $(DATA)/baseline $(MARK)

distclean: clean
	rm -rf $(DATA)/raw $(DATA)

diff-vs-shipped:
	$(PYTHON) 07_validate/diff_top100.py
