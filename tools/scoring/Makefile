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

.PHONY: rebuild validate clean distclean diff-vs-shipped help

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
	@echo "[02] extract (word, freq) per source"
	$(PYTHON) 02_extract/run_all.py
	@touch $@

03-normalize: $(MARK)/03.done
$(MARK)/03.done: $(MARK)/02.done
	@echo "[03] log-rank normalize per source"
	$(PYTHON) 03_normalize/run_all.py
	@touch $@

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
	       $(DATA)/merged $(DATA)/annotations $(MARK)

distclean: clean
	rm -rf $(DATA)/raw $(DATA)

diff-vs-shipped:
	$(PYTHON) 07_validate/diff_top100.py
