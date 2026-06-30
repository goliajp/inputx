# Autonomous execution protocol — dogfood loop

This file is the rulebook for each `/loop` fire while running the dogfood plan in `PLAN.md`.

## Hard rules

1. **Read PLAN.md first**, every fire. It's source of truth. Don't rely on chat memory.
2. **If `[PAUSE]` is on the first line** of PLAN.md → only print 1-line status (current todo count / last action) and exit. Do not advance.
3. **One item per fire** — find the **first** `[ ]` (skip `[WIP]` and `[BLOCKED:]`).
4. **Idempotency** — before starting an item, mark it `[WIP]` and commit (so a crash mid-iter doesn't lose the marker). After completion, mark `[x]` and commit.
5. **Append to Last action** — every fire writes timestamp + item id + 1-line summary to the "Last action" section.
6. **Commit hygiene** — every fire = 1-2 commits (`[WIP]` flip then `[x]` flip). Commit message: `dogfood: <item-id> <summary>`.
7. **If item fails or asks question** → mark `[BLOCKED: <one-line reason>]`,记录到 "Open questions",continue to next [ ] (if it has no dep on blocked item).
8. **Skip [WIP]** — if any item is already `[WIP]` from prior fire and >30 min old, assume stuck → mark `[BLOCKED: stuck mid-iter, investigate]`, move on.
9. **No scope expansion** — stick to PLAN items. If you discover a new task, add it to PLAN as a new `[ ]` row and continue with current item.
10. **All side-effect files in scratchpad** — corpus / segments / failures go under `docs/pinyin-dogfood-2026-06-30/scratchpad/`,not in chat. Reports go under `reports/`.

## Phase boundaries

- **End of a Phase**: 整个 Phase 全 `[x]` 时,在 Last action 写一句 "Phase N complete — N.N items / X commits / Y wall-clock min"。继续下一 Phase 第一 [ ]。
- **End of all Phases**: 写 "ALL DONE. dogfood concluded."。不再 schedule。`/loop` 系统自然停。

## Stop conditions

按 [[autorun_protocol]] 全套停下规则同步生效:

- Destructive action(rm -rf / force push / delete file > 100 行)→ stop, ask
- Scope expansion beyond PLAN(新 phase 整块加入)→ stop, ask  
- 用户级 API 调用 / 网络外部 fetch 之前 → stop, ask(corpus download 例外:THUCNews 这种 academic mirror OK,先告知 next iter)
- Baseline test regression > 1 fail → stop, investigate, ask
- 3 consecutive iters 同 item 仍未推进 → stop, mark [BLOCKED]

## Iter checklist (走流程,每 fire)

```
read PLAN.md
if [PAUSE] at top:
    print "PAUSED. last action: ..."
    exit
find next [ ] (not [WIP], not [BLOCKED])
if none: print "ALL DONE."; exit

item_id = the line id (e.g. "1.2")
mark [ ] → [WIP] in PLAN.md
git commit -m "dogfood: $item_id WIP <summary>" PLAN.md

execute item
  - tool calls as needed
  - all artifacts to scratchpad/ or proper repo locations
  - if item is multi-file edit, that's fine; one commit for all edits

mark [WIP] → [x] in PLAN.md
append to Last action: "<timestamp> $item_id done — <brief>"
git commit -A -m "dogfood: $item_id done <summary>"
git push origin develop

print 1-line status
```

## Anti-patterns

- ❌ 不读 PLAN.md 凭 chat memory 干活 → 会撞 race / 重做
- ❌ 一个 fire 干多个 item → commit 一团乱
- ❌ 静默改 PLAN 结构(加 phase / 重排 item)→ user 看不见
- ❌ 把 scratchpad 内容贴 chat → context 爆炸
- ❌ 跳 [PAUSE] 强行干 → 用户禁令
