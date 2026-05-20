# Inputx IME — App Store metadata

Item 95 of the iOS commercial-grade roadmap. App Store Connect submission
copy in zh-CN (primary) + en-US (secondary). Update before TestFlight
submission (item 99) and again before the public release (item 103).

---

## App basics

- **App name (zh-CN)**: Inputx 输入法
- **App name (en-US)**: Inputx IME
- **Subtitle (≤ 30 chars, zh-CN)**: 五笔 + 拼音 双引擎 · 自动识别
- **Subtitle (≤ 30 chars, en-US)**: Wubi + Pinyin dual-engine
- **Bundle ID**: `jp.golia.inputx`
- **Marketing version**: 1.0.0
- **Primary category**: Utilities
- **Secondary category**: Productivity
- **Age rating**: 4+

## Keywords (App Store Connect, ≤ 100 chars, comma-separated)

```
中文,输入法,五笔,拼音,IME,wubi,中文输入法,86版五笔,万能五笔,搜狗
```

(Note: keywords are bilingual-friendly. App Store Connect indexes by both
display language and keyword field; mixing 中文 + romanized aliases helps
discoverability for users searching either way.)

## Description (zh-CN, ≤ 4000 chars)

```
Inputx 输入法 —— 自研五笔 + 拼音双引擎中文输入法，纯本地，无网络无追踪。

主要特性：
• 五笔 86 编码全套：71 个键名字根 + 5,789 二级三级简码 + 61,205 词组，
  支持算法推导 + L0/L1+ 用户学习排序。
• 拼音输入：基于 Unihan + jieba 词库 + Leipzig + SUBTLEX-CH 语料构建，
  91 万 (拼音, 词) 候选条目。
• 万能 / 搜狗五笔风格的「混合模式」：以五笔为主，自动回落到拼音，
  无需手动切换输入方案；候选条用蓝 / 橙小点标识来源。
• 三种引擎模式可切换：混合 / 纯五笔 / 纯拼音。
• 智能学习：每选 3 次同一候选自动置顶（L0 用户偏好）；可在「设置」里
  导出 / 导入 / 重置学习数据。
• 中文标点自动转换：, → ， / . → 。 / ! → ！ / 智能引号自动配对等。
• 全宽切换：英文字母 / 数字一键变全宽（如 a → ａ）。
• 商业级 UX：候选条 W/P 来源指示、长按变体、自动大写、横屏支持、
  深色模式、Dynamic Type、VoiceOver 无障碍。
• 可选「显示罕用扩展字」（CJK Ext B+ 区罕用字）开关。

隐私：
- 完全本地运行，不联网，不收集任何数据。
- 不需要「允许完全访问」权限。
- L0 学习数据存在 App 本地，可随时一键重置或导出备份。

开源：
- 引擎代码完全开源（MIT / Apache-2.0 双许可）：
  - 五笔库：github.com/goliajp/wubi
  - 拼音库：github.com/goliajp/pinyin
- 数据来源透明，每个语料来源、字典、字体的许可在「设置 → 关于 →
  OSS 致谢」中列明。

设计目标：跟万能五笔 / 搜狗五笔 / Apple 拼音三家最好的部分对齐，做一个
干净、私有、可信赖的中文输入法。
```

## Description (en-US, ≤ 4000 chars)

```
Inputx IME — a self-built dual-engine Chinese input method (Wubi + Pinyin),
fully on-device, no network, no telemetry.

Key features:
• Wubi 86 encoding: 71 character roots + 5,789 simplified codes + 61,205
  phrases. Algorithmic decomposition + per-user L0/L1+ learning.
• Pinyin engine: Unihan + jieba dict + Leipzig + SUBTLEX-CH corpora.
  919k (pinyin, word) candidate entries.
• "Mixed mode" (万能/搜狗-style): Wubi-primary with automatic Pinyin
  fallback. No manual switching needed. Candidate bar marks source via a
  blue (Wubi) or orange (Pinyin) dot.
• Three switchable modes: Mixed / Wubi-only / Pinyin-only.
• User learning: 3-pick auto-promotion of preferred candidates (L0).
  Export / import / reset L0 from Settings.
• Chinese punctuation auto-conversion (`, → ，`, `. → 。`, smart quote
  pairing, etc.). Full-width letter / digit toggle.
• Commercial-grade UX: W/P source indicator, long-press variants,
  AutoCaps, landscape, dark mode, Dynamic Type, VoiceOver.
• Optional "show rare CJK extension chars" toggle for power users.

Privacy:
- Fully local. No network. No data collection.
- "Allow Full Access" not required.
- L0 learning state lives only on-device; export or reset from Settings.

Open source:
- Engine code dual-licensed MIT / Apache-2.0:
  - Wubi: github.com/goliajp/wubi
  - Pinyin: github.com/goliajp/pinyin
- Every corpus / dictionary / font source is credited in
  Settings → About → OSS acknowledgments.

Design goal: take the best parts of 万能五笔, 搜狗五笔, and Apple Pinyin —
deliver a clean, private, trustworthy Chinese IME.
```

## What's New (release notes for v1.0.0, ≤ 4000 chars per locale)

### zh-CN
```
v1.0 首版发布。

- 五笔 + 拼音双引擎（混合 / 纯五笔 / 纯拼音三种模式）。
- 万能五笔风格的自动识别：五笔解不出时自动回落到拼音。
- 候选条 W/P 来源指示，长按看 Unihan 信息。
- 中文标点自动转换 + 智能引号 + 全宽切换。
- 用户学习数据 (L0) 可导出 / 导入 / 重置。
- 完全本地运行，不联网，不需要「允许完全访问」。
```

### en-US
```
v1.0 initial release.

- Dual-engine: Wubi + Pinyin (Mixed / Wubi-only / Pinyin-only modes).
- Auto-detection 万能五笔-style: Pinyin fallback when Wubi yields empty.
- Candidate bar W/P source dot, long-press for Unihan info.
- Chinese punctuation auto-convert + smart quotes + full-width toggle.
- User learning data (L0) export / import / reset from Settings.
- Fully local. No network. "Allow Full Access" not required.
```

## App Review notes (private; for App Store Connect's review-notes field)

```
Engine details for the reviewer:

1. Wubi 86 reference: https://en.wikipedia.org/wiki/Wubi_method . Inputx's
   wubi engine is self-built on the standard 86 encoding (the same
   encoding used by 万能/搜狗/Apple's own wubi IME). No GPL or
   copyleft data — all engine code is dual-licensed MIT/Apache-2.0,
   published openly at github.com/goliajp/wubi.

2. Pinyin engine: also self-built (github.com/goliajp/pinyin). Single-
   character readings sourced from the Unicode Unihan database (PD via
   Unicode License v3); phrase list from jieba (MIT, fxsjy/jieba); word
   frequency from Leipzig Corpora Collection + SUBTLEX-CH (both CC-BY 4.0).
   No CC-BY-SA / GPL / unclear-provenance data ships.

3. Network usage: ZERO. The app contains no networking code. Verify by
   checking entitlements (no com.apple.security.network.* entitlements)
   and by scanning the binary for URLSession / NSURLConnection / etc.
   The keyboard extension also makes no network calls.

4. "Allow Full Access" prompt: NOT requested. The keyboard works without
   it. The bundled font fallback ("Inputx CJK Extended", subset of
   Plangothic Super, OFL 1.1) lives inside the keyboard extension's
   bundle so no system-wide font registration is needed.

5. Bundled mobileconfig profile: there's an optional system-font install
   button under Settings → 显示 → "在 Safari 中下载 Profile". This
   downloads a configuration profile from
   github.com/goliajp/inputx releases (signed by an Apple
   Development cert; chain incomplete = system shows "尚未验证"). The
   user must manually install in Settings → General → VPN & Device
   Management. Even after install, third-party apps may not pick up the
   font — Apple's font fallback chain is platform policy. We document
   this honestly in the UI.

Test account: not needed. App opens directly to the settings UI; no
login.

If you have any concerns about the dual-engine implementation or any
licensing question, please reach out to admin@golia.jp before rejecting —
we're happy to provide additional source pointers.
```

## Tester notes (TestFlight item 99 internal beta)

- Most likely "broken" experience: long-press behavior on iOS 16 vs iOS 17
  may render variant popovers slightly differently. Report any layout
  weirdness with screenshot + iOS version.
- Test L0 export → reset → import on the same device to confirm the
  round-trip preserves your pinned candidates.
- Expected v1 limitation: the long-press popover on letter keys only
  shows uppercase variant for now (accented Latin variants land in v1.1).
- "落实 / 老师" type heteronym ranking: documented v0.2 limitation. Top
  candidate may not be the one you'd type 90% of the time; that's
  what L0 user-learning fixes after a few uses.
