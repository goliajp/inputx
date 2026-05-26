# Inputx IME — App Store submission checklist

This consolidates the user-action items from ROADMAP Phase 10–11 (items
91–105). Open it in App Store Connect alongside; copy-paste the canned
answers / paste the screenshot positions / etc.

Companion files:
- `AppStore-metadata.md` — full bilingual app description, keywords,
  release notes, reviewer notes (item 95).
- `PrivacyPolicy.md` — privacy policy markdown (item 92).
- `App/Assets.xcassets/AppIcon.appiconset/` — generated icon set
  (items 88, 90).

---

## Before TestFlight (Phase 10 items 91-94)

### Item 91 — Performance budgets verification 〚requires device〛

Run these on the test device via Instruments before TestFlight upload.
Document any miss in this file (so v1.0.x patch knows to address).

| Budget                                                  | Target           | Measured | Pass? |
|---------------------------------------------------------|------------------|----------|-------|
| Cold-start (InputxApp launch → SettingsView visible)      | < 300ms          | device-TBD | —   |
| Cold-start (Keyboard appex → first paint)               | < 500ms          | device-TBD | —   |
| Keystroke → candidate refresh latency (p99)             | < 50ms           | engine p95<0.3ms (host perfgate) ✓; UI device-TBD | ✓* |
| Idle resident memory (Keyboard extension)               | < 50MB           | ~23M dict rodata+binary; device-TBD | ⚠ near-limit |
| First-keystroke latency after keyboard show             | < 100ms          | device-TBD | —   |
| 60fps scrolling with 50 candidates (item 60)            | no dropped frame | device-TBD | —   |

#### CP2 baseline (v1.2, 2026-05-25, post-hybrid-cutover)
- **Embedded pinyin data** (`include_bytes!`): pinyin.dict 4.2M + bigrams.fsa 4.3M
  + bigrams_intra.fsa 1.4M + **trigrams.dict 13M** = **23M total**. trigrams is 57%
  → primary target for compression / on-demand layering (latter = CP6 sideload).
- **Engine perfgate** (release, `scripts/perf_isolated.sh`): every keystroke
  < 0.3ms worst-case (nihaomawojiao max 0.30ms) vs the 16ms frame budget — the
  Rust hot path is NOT a bottleneck. (UI-layer refresh/scroll still device-TBD.)
- Device rows (cold-start, RSS, fps) need real-iPhone Instruments — deferred to
  user's device per the sim-default workflow.

Tools:
- Instruments → Time Profiler for cold-start + keystroke latency
- Instruments → Allocations / VM Tracker for memory
- Instruments → Animation Hitches for scroll fps

### Item 92 — Privacy policy hosting

Source: `ios/PrivacyPolicy.md`. Two host options:

**Option A — golia.jp (preferred for branding)**:
1. Convert markdown → HTML (e.g., via `pandoc PrivacyPolicy.md -o privacy.html`).
2. Push to whatever serves golia.jp (web host config out of scope for
   this checklist).
3. Final URL: `https://golia.jp/inputx/privacy` (the URL the App's
   Settings → 关于 → 隐私政策 link points at).

**Option B — GitHub Pages (fastest)**:
1. New repo: `goliajp/inputx-privacy`.
2. Push `PrivacyPolicy.md` as `index.md`.
3. Repo Settings → Pages → enable from `main` branch root.
4. Final URL: `https://goliajp.github.io/inputx-privacy/`.
5. Update SettingsView's `Link(destination: URL(string: "...")!)` to
   the new URL OR add a redirect at golia.jp/inputx/privacy →
   github.io URL.

App Store Connect also lets you paste the policy text directly into a
textarea — useful as a backup if hosting setup slips before submission
deadline.

### Item 93 — App Store Connect privacy nutrition label

App Store Connect → Inputx IME → App Privacy → Get Started.

**Question 1: Does this app collect any data from this app?**
- Answer: **No, we do not collect data from this app**

That's it. Confirm + Save. Apple shows "Data Not Collected" badge on
the App Store listing.

(If Apple Developer changes the form to require detail-by-default, the
canned answers are: no contact info, no health, no financial, no
location, no sensitive info, no contacts, no user content, no browsing
history, no search history, no identifiers, no purchases, no usage data,
no diagnostics. Crash reports = "Diagnostics, anonymized via Apple, not
linked to user, not used for advertising or product personalization.")

### Item 94 — Screenshots 〚requires device〛

Required sizes per Apple HIG: **iPhone 6.7"** (e.g., iPhone 15 Pro Max,
1290×2796px) AND **iPhone 6.1"** (e.g., iPhone 15, 1179×2556px). Take
screenshots on the test device (e.g. iPhone 15 Pro), then optionally use Sketch /
Figma to scale + relayout for the 6.7" frame. (App Store accepts
identical screenshots scaled to both sizes — same content.)

**Required: 4–6 screenshots**. Recommended set:

| # | Subject                                | Caption (zh-CN)                   | Caption (en-US)                      |
|---|----------------------------------------|-----------------------------------|--------------------------------------|
| 1 | Notes app + Inputx keyboard, typing `khlg` showing 中国 candidate w/ blue W dot | 五笔 + 拼音 自动识别 | Wubi + Pinyin auto-detected |
| 2 | Same scene typing `zhongguo` showing 中国 candidate w/ orange P dot | 拼音 fallback 不用切换 | Pinyin fallback, no switching |
| 3 | Settings → 输入方案 + 输入行为 sections | 三种引擎模式 + 自动提交策略 | Three engine modes + auto-commit |
| 4 | Settings → 显示 + 中文输入习惯 sections | 中文标点 + 全宽 + 罕用字 | CJK punct + full-width + rare chars |
| 5 | Settings → 学习与个性化 (export sheet visible) | 学习数据 export / import / reset | Export, import, or reset learning data |
| 6 | About + 致谢 page (showing 7 OSS credits) | 完全开源 + 隐私优先 | Fully open source + privacy first |

Capture flow:
1. Set device language to zh-CN, take all 6 zh shots.
2. Set device language to en, take all 6 en shots.
3. Upload to App Store Connect → App Information → Localizations →
   each language → Screenshots.

---

## TestFlight (Phase 11 items 98-99)

### Item 98 — Crash log monitoring

**No code changes needed** — Apple's pipeline auto-collects crash
reports from TestFlight + production users (subject to opt-in via iOS
Settings → Privacy & Security → Analytics & Improvements → Share with
App Developers).

Verification flow:
1. Build with debug symbols (already on for Debug; Release config in
   project.yml inherits dwarf-with-dsym for Archive builds).
2. Archive in Xcode (`xcodebuild archive ...` or Xcode menu Product →
   Archive).
3. Upload to App Store Connect via Organizer or `xcodebuild -exportOptionsPlist`.
4. Apple processes the IPA + dSYMs server-side; crashes appear in:
   - **Xcode → Window → Organizer → Crashes** (preferred — symbolicated)
   - **App Store Connect → Inputx IME → Analytics → Crashes**

If symbolication fails (you see hex addresses instead of source lines),
manually upload dSYMs:
- App Store Connect → My Apps → Inputx IME → TestFlight → [build] →
  Activity → Build Metadata → Include Symbols
- Or `xcrun altool --upload-symbols ...`

Inputx specifically has zero networking code, so crashes related to
network, auth, or third-party SDKs literally cannot happen. Most likely
crash sources to watch:
- InputxCore static lib `inputx_core`'s Rust panics (would surface as
  `EXC_BREAKPOINT (SIGTRAP)` in the CompositeEngine or wubi-side code).
- KeyboardViewController layout edge cases (Constraint conflicts).
- Memory pressure in extension (50MB extension limit).

### Item 99 — TestFlight beta

Required:
- Apple Dev account active + paid ($99/yr).
- App Store Connect record created (Inputx IME, bundle ID
  `jp.golia.inputx`).
- App Store Connect agreements + tax + banking forms complete.

Flow:
1. Bump `CURRENT_PROJECT_VERSION` in project.yml if not done already
   (each new TF upload needs a higher build number).
2. `cd ios && xcodebuild -scheme InputxApp -configuration Release \
   -destination 'generic/platform=iOS' archive \
   -archivePath build/InputxApp.xcarchive` (or Xcode → Product → Archive).
3. `xcodebuild -exportArchive -archivePath build/InputxApp.xcarchive \
   -exportPath build/export -exportOptionsPlist ExportOptions.plist`
   (you'll need to write ExportOptions.plist; see Apple docs).
4. Upload via `xcrun altool --upload-app` OR Xcode Organizer.
5. Apple processes (~15-60 min). When status = "Ready to Test",
   add testers (App Store Connect → TestFlight → Internal Testing).
6. ≥ 1 outside tester for ≥ 1 week (per ROADMAP).

---

## Submit for review (Phase 11 items 101-103)

### Item 101 — Submit

App Store Connect → Inputx IME → App Store → "Submit for Review".

**Required answers**:
- **Sign-in required?** No.
- **Contact info for reviewer**: admin@golia.jp.
- **Demo account**: Not required.
- **Notes for reviewer**: paste from `AppStore-metadata.md` "App Review
  notes" section. Critical points:
  1. Inputx doesn't request "Allow Full Access" — confirm in entitlements.
  2. Zero network. No third-party SDK.
  3. Wubi 86 reference is public domain encoding; engine self-built
     under MIT/Apache.
  4. Optional config-profile system font is a separate user-driven
     workflow Apple already supports; we don't bypass anything.

### Item 102 — Reviewer feedback

Common keyboard-extension review bounces (be ready):
- "Allow Full Access" prompt UX. We don't request it; if reviewer
  insists on a prompt explaining, point at `KeyboardViewController.swift`
  showing the keyboard works without it.
- "Localization complete?" — Yes, en + zh-Hans declared.
- "Privacy Policy URL not reachable" — make sure it resolves before
  submission.

### Item 103 — Release

After approval, set "Manual release" → click "Release this version" when
ready. App goes live in 24h on App Store search.

---

## Post-release (items 104-105)

- Watch Crashes daily for first week.
- Respond to App Store reviews via App Store Connect → Ratings &
  Reviews.
- If a critical bug surfaces, queue v1.0.1 with a `CURRENT_PROJECT_VERSION`
  bump.
- Item 105 retro: write up what landed easily, what was painful, what
  v1.1 should fix. Drop into `~/workspace/labs/inputx/v1-retro.md` or
  similar.
