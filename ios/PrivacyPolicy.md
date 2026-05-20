# Inputx IME — Privacy Policy

**Effective date**: 2026-05-11
**Last updated**: 2026-05-11
**Publisher**: GOLIA K.K. (Japan)
**Contact**: admin@golia.jp

This privacy policy describes how Inputx IME ("the App") handles user data.
The short version: **Inputx IME does not collect, transmit, or share any
personal data. Everything stays on your device.**

---

## In plain language

- **No network**. The App contains no networking code. It cannot send
  data anywhere because it cannot make network requests.
- **No analytics**. We do not use any analytics SDK (no Firebase, no
  Mixpanel, no custom telemetry).
- **No accounts**. The App has no login, no user account, no profile.
- **No "Allow Full Access"**. Inputx keyboard works without iOS's "Allow
  Full Access" permission. We never request it.
- **What stays on your device**: your typing-pattern learning state
  (called "L0" — see below). You can export it as a JSON file you own,
  or wipe it from Settings at any time.

---

## What data Inputx stores on your device

### Learning data (L0)

When you type and pick candidates from Inputx's candidate bar, the engine
records which candidates you preferred for each input string. After you
pick the same candidate three times for the same input, that candidate
auto-promotes to position 0 for that input.

This learning state lives in two JSON files inside the iOS App Group
container `group.jp.golia.inputx`, at:

```
Library/Application Support/inputx/
    wubi_l0.json      (Wubi engine pins + pick counters)
    pinyin_l0.json    (Pinyin engine pins + pick counters)
```

This data **never leaves your device** unless you explicitly export it
yourself via **Settings → 学习与个性化 → 导出学习数据**, which gives you
the JSON files via the standard iOS share sheet (Save to Files, AirDrop,
etc.). You can also reset it via **Settings → 学习与个性化 → 重置学习
数据**, which deletes both files.

The JSON contains only:
- The text you typed (e.g., `"khlg"`, `"zhongguo"`)
- The Chinese candidate you selected (e.g., `"中国"`)
- Counter integers tracking pick frequency

It does NOT contain:
- The full text of anything you've typed elsewhere
- Timestamps
- Any identifier for you or your device

### User preferences

These four toggle settings persist in iOS's App Group `UserDefaults`:

- `engineMode` — Mixed / Wubi-only / Pinyin-only
- `showRareChars` — toggle for rare CJK characters
- `showSourceIndicator` — toggle for the W/P dot in candidate bar
- `autoCommitPolicy` — when to auto-commit unique 4-letter codes
- `useCjkPunct` — convert ASCII punct to CJK forms in Chinese mode
- `useFullWidth` — full-width letters / digits

These preferences are local-only.

---

## What Inputx does NOT do

- **No keystroke logging**. iOS keyboard extensions can technically log
  every keystroke if granted "Allow Full Access". Inputx does not request
  this permission and so cannot read what you type into other apps. We
  only see the keystrokes that go through Inputx's own input view.
- **No data transmission**. The App and keyboard extension contain no
  networking code. Audit the binary or check `Info.plist` /
  entitlements — there are no `com.apple.security.network.*`
  entitlements set.
- **No third-party SDKs**. Inputx ships only Apple's standard frameworks
  (UIKit, SwiftUI, etc.) plus our own engine code (links open at
  Settings → 关于 → 源代码).

---

## "Allow Full Access" permission

Inputx IME does not request the iOS keyboard extension's "Allow Full
Access" permission. This means the keyboard cannot:

- Read text from the surrounding text field beyond what's needed to
  insert candidates
- Make network requests
- Access shared resources outside its sandbox

If iOS prompts you to grant this permission, that's a system-level
suggestion — Inputx will not ask for it during normal use, and the IME
works fully without it.

---

## Optional system-font configuration profile

Under **Settings → 显示 → 显示罕用扩展字 → 在 Safari 中下载 Profile**,
the App offers an optional system-wide font installation via Apple's
configuration profile mechanism. This:

- Downloads `InputxCJKExtended.mobileconfig` from
  https://github.com/goliajp/inputx/releases (a public GitHub
  repository release hosting the signed profile).
- Hands the file off to iOS Settings, where you must manually approve
  installation under **Settings → General → VPN & Device Management**.
- Even if installed, the font may not be picked up by all third-party
  apps' fallback chains — this is an Apple platform limitation.

The download is a standard HTTPS GET to GitHub, performed by Safari (not
by Inputx). GitHub's privacy policy applies to that download:
https://docs.github.com/en/site-policy/privacy-policies/github-privacy-statement

Inputx itself does not hit any servers.

---

## Data Inputx IME does NOT collect

For App Store Privacy nutrition label completeness:

- Contact info — none
- Health & fitness — none
- Financial info — none
- Location — none
- Sensitive info — none
- Contacts — none
- User content — none (we don't read your typing in other apps)
- Browsing history — none
- Search history — none
- Identifiers — none
- Purchases — none
- Usage data — none
- Diagnostics — only Apple's own anonymized crash reports (see below)

## Crash reports

If you have iOS's "Share with App Developers" setting on (Settings →
Privacy & Security → Analytics & Improvements), Apple may send Inputx
crash reports through their own anonymized infrastructure. These reports
contain stack traces and device metadata but no user content, no input
text, and no identifiers. We use these only to fix bugs.

You can opt out at any time via the iOS Settings path above.

---

## Children's privacy

Inputx IME does not knowingly collect any data from anyone (children
included), as it does not collect data at all.

---

## Changes to this policy

We will publish updates to this policy at the same URL where you found
this version (linked from **Settings → 关于 → 隐私政策**). Material
changes (e.g., if we ever add network features in a future version)
will be highlighted in the App's release notes.

---

## Contact

Questions or concerns about this policy:

- Email: **admin@golia.jp**
- GitHub: https://github.com/goliajp (issues welcome on relevant repos)

---

*This policy is published under CC0 (public domain) so other
privacy-conscious app authors can reuse the structure.*
