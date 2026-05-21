import SwiftUI
import UIKit
import UniformTypeIdentifiers
import InputxKit

/// Non-optional UserDefaults reference for SwiftUI `@AppStorage`. Falls
/// back to `.standard` if the App Group entitlement is somehow missing
/// (defensive — shouldn't happen in production).
// Backward-compat alias: pre-refactor the keyboard's `inputxSharedDefaults`
// was an `UserDefaults?`. Globals.swift now exposes a non-optional value
// with a `.standard` fallback baked in, so this alias is identity.
let inputxSharedDefaultsNonOpt: UserDefaults = inputxSharedDefaults

/// Phase 8 (item 70-75) — top-level InputxApp settings UI, SwiftUI rewrite of
/// the v0.1 UIKit-built `MainViewController`. Five sections:
///   1. Engine mode picker (item 71)
///   2. Display (item 72) — rare-char, source dot, cascade font installer
///   3. Behavior (item 73) — AutoCommitPolicy
///   4. Learning (item 74) — L0 import / export / reset
///   5. About (item 75) — version, licenses, github, privacy
struct SettingsView: View {
    @AppStorage("engineMode",          store: inputxSharedDefaultsNonOpt) private var engineMode: Int = 0
    @AppStorage("showRareChars",       store: inputxSharedDefaultsNonOpt) private var showRare: Bool = false
    @AppStorage("showSourceIndicator", store: inputxSharedDefaultsNonOpt) private var showSrc: Bool = true
    // Default 0 (Never) — see InputxSettings.registerDefaults() rationale.
    @AppStorage("autoCommitPolicy",    store: inputxSharedDefaultsNonOpt) private var autoCommitPolicy: Int = 0
    // Phase 9 (items 84-85) — locale toggles
    @AppStorage("useCjkPunct",         store: inputxSharedDefaultsNonOpt) private var useCjkPunct: Bool = true
    @AppStorage("useFullWidth",        store: inputxSharedDefaultsNonOpt) private var useFullWidth: Bool = false
    /// JP plugin enhancement toggle — orthogonal to engineMode. When
    /// engineMode == .japaneseOnly (3), this toggle is moot (mode forces JP).
    /// Default true — see InputxSettings.registerDefaults().
    @AppStorage("japaneseEnabled",     store: inputxSharedDefaultsNonOpt) private var japaneseEnabled: Bool = true

    @State private var showResetConfirm = false
    @State private var showImportPicker = false
    @State private var showShareSheet = false
    @State private var shareItems: [Any] = []
    @State private var infoBanner: String?
    /// Scratch field for testing the Inputx keyboard without leaving the app —
    /// faster dev loop than switching to Notes/Safari every time.
    @State private var debugText: String = ""
    @FocusState private var debugFocused: Bool

    var body: some View {
        NavigationStack {
            Form {
                Section("启用步骤") {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("1. 设置 → 通用 → 键盘 → 键盘")
                        Text("2. 添加新键盘 → 选「Inputx」")
                        Text("3. 在任意文本框点 🌐 切到 Inputx")
                        Text("v1 暂不需要「允许完全访问」。")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .padding(.top, 4)
                    }
                    .font(.subheadline)
                }

                // ---------- 调试输入框 + 内嵌键盘 ----------
                // The in-app keyboard hosts a live KeyboardViewController +
                // injects a fake text proxy that writes into $debugText,
                // bypassing iOS's third-party-keyboard activation gate that
                // blocks reliable maestro testing of the real extension
                // (item 92). What's tested here = what users get in the
                // real keyboard — same source compiled into both targets.
                Section {
                    Text(debugText.isEmpty ? "（按下方键盘开始输入）" : debugText)
                        .font(.body)
                        .foregroundStyle(debugText.isEmpty ? .secondary : .primary)
                        .frame(maxWidth: .infinity, minHeight: 80, alignment: .topLeading)
                        .padding(.vertical, 4)
                        .accessibilityIdentifier("debug-text-area")
                    InAppKeyboardView(text: $debugText)
                        .frame(height: 290)
                        .accessibilityIdentifier("in-app-keyboard")
                    HStack {
                        Text("\(debugText.count) 字")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        Spacer()
                        if !debugText.isEmpty {
                            Button(role: .destructive) {
                                debugText = ""
                            } label: {
                                Label("清空", systemImage: "trash")
                                    .labelStyle(.titleAndIcon)
                                    .font(.callout)
                            }
                        }
                    }
                } header: {
                    Text("调试键盘")
                } footer: {
                    Text("内嵌的 Inputx 键盘——直接按键测试，不用切到系统第三方键盘。这里跟真键盘共享同一份代码，行为一致。")
                }

                // ---------- 1. Engine mode (item 71) ----------
                Section {
                    Picker("引擎模式", selection: $engineMode) {
                        Text("混合").tag(0).accessibilityIdentifier("engine-mode-mixed")
                        Text("五笔").tag(1).accessibilityIdentifier("engine-mode-wubi")
                        Text("拼音").tag(2).accessibilityIdentifier("engine-mode-pinyin")
                        Text("日语").tag(3).accessibilityIdentifier("engine-mode-japanese")
                    }
                    .pickerStyle(.menu)
                    .accessibilityIdentifier("picker-engine-mode")
                    // JP plugin enhancement toggle. Hidden when the picker
                    // is already on .japaneseOnly (mode forces JP, toggle
                    // would just confuse).
                    if engineMode != 3 {
                        Toggle("日语扩展（候选追加假名 + 共形汉字）", isOn: $japaneseEnabled)
                            .accessibilityIdentifier("toggle-japanese-enabled")
                    }
                    Text(engineModeNote)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                } header: {
                    Text("输入方案")
                } footer: {
                    Text("切换后请关闭再重新打开 Inputx 键盘以生效（v1 不支持 mid-session 切换）。日语扩展默认关闭——打开后输入罗马字时会在中文候选之后追加平假名 / 片假名 / 与简体共形的汉字。")
                }

                // ---------- 2. Display (item 72) ----------
                Section("显示") {
                    Toggle("候选条显示 W / P 来源点", isOn: $showSrc)
                        .accessibilityIdentifier("toggle-show-source")
                    Toggle("显示罕用扩展字（CJK Ext B+）", isOn: $showRare)
                        .accessibilityIdentifier("toggle-show-rare")
                    if showRare {
                        VStack(alignment: .leading, spacing: 6) {
                            Text("⚠️ 罕用字在 Inputx 候选条里能渲染（keyboard 内带字体），但 commit 到第三方 app 后会显示 ?。可选装系统字体（实验性）：")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            Button("在 Safari 中下载 Profile") {
                                openProfile()
                            }
                            .font(.callout)
                        }
                        .padding(.vertical, 4)
                    }
                }

                // ---------- 3. Behavior (item 73) ----------
                Section {
                    Picker("自动提交策略", selection: $autoCommitPolicy) {
                        Text("从不自动").tag(0).accessibilityIdentifier("policy-never")
                        Text("满 4 码").tag(1).accessibilityIdentifier("policy-four")
                        Text("仅唯一候选").tag(2).accessibilityIdentifier("policy-unique")
                        Text("满 4 码且唯一（默认）").tag(3).accessibilityIdentifier("policy-default")
                    }
                    .pickerStyle(.menu)
                    .accessibilityIdentifier("picker-autocommit")
                } header: {
                    Text("输入行为")
                } footer: {
                    Text("控制 Inputx 何时不等用户选择就直接 commit 顶部候选。")
                }

                // ---------- 3b. Locale (Phase 9 items 84-85) ----------
                Section {
                    Toggle("中文标点", isOn: $useCjkPunct)
                        .accessibilityIdentifier("toggle-cjk-punct")
                    Toggle("英文 / 数字全宽", isOn: $useFullWidth)
                        .accessibilityIdentifier("toggle-full-width")
                } header: {
                    Text("中文输入习惯")
                } footer: {
                    Text("中文标点：键盘上的 , . ! ? 等会自动映射为 ， 。 ！ ？。智能引号根据上下文交替。\n全宽：英文字母 / 数字插入时变成全宽形式（如 a → ａ），CJK 排版常用。")
                }

                // ---------- 4. Learning (item 74) ----------
                Section {
                    Button {
                        exportL0()
                    } label: {
                        Label("导出学习数据", systemImage: "square.and.arrow.up")
                    }
                    Button {
                        showImportPicker = true
                    } label: {
                        Label("导入学习数据", systemImage: "square.and.arrow.down")
                    }
                    Button(role: .destructive) {
                        showResetConfirm = true
                    } label: {
                        Label("重置学习数据", systemImage: "trash")
                    }
                } header: {
                    Text("学习与个性化")
                } footer: {
                    Text("L0 是 Inputx 跟踪你常用候选的内部状态（每选 3 次自动置顶）。导出 / 导入是为了换设备或备份；重置会清空两个引擎的全部用户偏好。")
                }

                // ---------- 5. About (item 75) ----------
                Section("关于") {
                    HStack {
                        Text("版本")
                        Spacer()
                        Text(appVersion).foregroundStyle(.secondary)
                    }
                    Link(destination: URL(string: "https://github.com/goliajp")!) {
                        Label("源代码 (GitHub)", systemImage: "chevron.left.forwardslash.chevron.right")
                    }
                    Link(destination: URL(string: "https://golia.jp/inputx/privacy")!) {
                        Label("隐私政策", systemImage: "hand.raised")
                    }
                    NavigationLink {
                        AcknowledgementsView()
                    } label: {
                        Label("OSS 致谢", systemImage: "doc.text")
                    }
                }
            }
            .navigationTitle("Inputx 输入法")
            .alert("重置全部学习数据？", isPresented: $showResetConfirm) {
                Button("取消", role: .cancel) {}
                Button("重置", role: .destructive) {
                    resetL0()
                }
            } message: {
                Text("会删除两个引擎（五笔 + 拼音）的全部 L0 pin 和 pick counters。重置后 Inputx 会回到出厂的频率排序。下次打开键盘生效。此操作不可撤销。")
            }
            .sheet(isPresented: $showShareSheet) {
                ShareSheet(items: shareItems)
            }
            .fileImporter(
                isPresented: $showImportPicker,
                allowedContentTypes: [.json],
                allowsMultipleSelection: true
            ) { result in
                handleImportResult(result)
            }
            .overlay(alignment: .top) {
                if let banner = infoBanner {
                    Text(banner)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 10)
                        .background(.thinMaterial)
                        .clipShape(RoundedRectangle(cornerRadius: 12))
                        .padding(.top, 8)
                        .transition(.move(edge: .top).combined(with: .opacity))
                }
            }
        }
    }

    // MARK: - Computed strings

    private var engineModeNote: String {
        switch engineMode {
        case 1: return "纯五笔输入。"
        case 2: return "纯拼音输入。"
        case 3: return "纯日语输入：罗马字 → 平假名 / 片假名 / 共形汉字。中文引擎关闭。"
        default: return "五笔为主，自动识别拼音 fallback（万能 / 搜狗五笔风格）。"
        }
    }

    private var appVersion: String {
        let v = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "?"
        let b = Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? "?"
        return "\(v) (build \(b))"
    }

    // MARK: - L0 actions

    private func exportL0() {
        // Share the on-disk JSON files directly so iOS's share sheet picks
        // up the right MIME type. The InputxL0Storage instance exposes its
        // directory so we can build per-engine URLs without re-encoding the
        // file-name policy here.
        let dir = inputxL0Storage.directoryURL
        var urls: [URL] = []
        for (engine, name) in [(InputxL0Engine.wubi, "wubi_l0.json"),
                               (.pinyin, "pinyin_l0.json")] {
            let url = dir.appendingPathComponent(name)
            if FileManager.default.fileExists(atPath: url.path) {
                urls.append(url)
            }
            _ = engine // satisfy unused-binding lint
        }
        if urls.isEmpty {
            flashBanner("还没有学习数据。在键盘里多打几次试试。")
            return
        }
        shareItems = urls
        showShareSheet = true
    }

    private func handleImportResult(_ result: Result<[URL], Error>) {
        switch result {
        case .failure(let err):
            flashBanner("导入失败: \(err.localizedDescription)")
        case .success(let urls):
            var imported = 0
            for url in urls {
                let needsRelease = url.startAccessingSecurityScopedResource()
                defer { if needsRelease { url.stopAccessingSecurityScopedResource() } }
                guard let data = try? String(contentsOf: url, encoding: .utf8) else { continue }
                // Best-effort detect: "engine":"wubi" or "engine":"pinyin"
                let engine: InputxL0Engine?
                if data.contains("\"engine\":\"wubi\"") {
                    engine = .wubi
                } else if data.contains("\"engine\":\"pinyin\"") {
                    engine = .pinyin
                } else {
                    engine = nil
                }
                if let e = engine, inputxL0Storage.writeJson(data, engine: e) {
                    imported += 1
                }
            }
            if imported > 0 {
                flashBanner("导入了 \(imported) 个学习数据文件。下次打开键盘生效。")
            } else {
                flashBanner("没有识别到有效的 Inputx L0 JSON。")
            }
        }
    }

    private func resetL0() {
        // Check existence before reset so we can give the right message.
        let dir = inputxL0Storage.directoryURL
        let any = ["wubi_l0.json", "pinyin_l0.json"].contains { name in
            FileManager.default.fileExists(atPath: dir.appendingPathComponent(name).path)
        }
        inputxL0Storage.reset()
        flashBanner(any ? "学习数据已重置。" : "本来就没有学习数据。")
    }

    private func openProfile() {
        let url = URL(string: "https://github.com/goliajp/inputx/releases/download/v1.0.0/InputxCJKExtended.mobileconfig")!
        UIApplication.shared.open(url)
    }

    private func flashBanner(_ text: String) {
        withAnimation { infoBanner = text }
        DispatchQueue.main.asyncAfter(deadline: .now() + 2.5) {
            withAnimation { infoBanner = nil }
        }
    }
}

// MARK: - Share sheet wrapper

private struct ShareSheet: UIViewControllerRepresentable {
    let items: [Any]
    func makeUIViewController(context: Context) -> UIActivityViewController {
        UIActivityViewController(activityItems: items, applicationActivities: nil)
    }
    func updateUIViewController(_ vc: UIActivityViewController, context: Context) {}
}

// MARK: - Acknowledgements (item 96 partial — full OSS list)

struct AcknowledgementsView: View {
    var body: some View {
        Form {
            Section("引擎数据") {
                creditRow(name: "Unihan Database", license: "Unicode License v3", url: "https://www.unicode.org/license.txt")
                creditRow(name: "jieba dict.txt", license: "MIT, © 2013 Sun Junyi", url: "https://github.com/fxsjy/jieba")
                creditRow(name: "Leipzig Corpora Collection", license: "CC-BY 4.0", url: "https://wortschatz-leipzig.de/en/download/Chinese")
                creditRow(name: "SUBTLEX-CH-WF", license: "CC-BY 4.0 (Cai & Brysbaert 2010)", url: "https://doi.org/10.1371/journal.pone.0010729")
            }
            Section("引擎代码") {
                creditRow(name: "inputx-wubi (五笔 86)", license: "MIT or Apache-2.0, GOLIA K.K.", url: "https://crates.io/crates/inputx-wubi")
                creditRow(name: "inputx-pinyin", license: "MIT or Apache-2.0, GOLIA K.K.", url: "https://crates.io/crates/inputx-pinyin")
            }
            Section("字体") {
                creditRow(name: "Inputx CJK Extended (Plangothic Super P1 subset)", license: "OFL 1.1, renamed per §1 RFN clause", url: "https://github.com/goliajp/inputx")
            }
        }
        .navigationTitle("OSS 致谢")
    }

    private func creditRow(name: String, license: String, url: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(name).font(.subheadline).fontWeight(.medium)
            Text(license).font(.caption).foregroundStyle(.secondary)
            if let u = URL(string: url) {
                Link(url, destination: u).font(.caption2)
            }
        }
        .padding(.vertical, 2)
    }
}

#Preview {
    SettingsView()
}
