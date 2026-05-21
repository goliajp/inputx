import AppKit
import SwiftUI
import InputxKit

/// First-class macOS settings window for Inputx — opened from the
/// menubar status item's "设置…" entry. Mirrors the iOS container app's
/// SettingsView surface so users get the same toggles on both platforms.
///
/// Why a real window and not just the dropdown? Two pain points
/// observed during JP-plugin landing:
///   - macOS's *system* input-source switcher in the menu bar uses the
///     same "入" glyph as Inputx's own NSStatusItem. Users click the
///     wrong one and never find our config.
///   - Auto-hidden menu bars (a common ergonomics choice) bury both
///     icons until the cursor approaches the top of the screen. Even
///     with the right click, the discoverability is poor.
///
/// A proper Settings window solves both: the user opens it once from
/// menubar (or future Cmd+, in a keyboard window), then has every
/// toggle on screen at the same time.
///
/// The window broadcasts `inputxSettingsChanged` whenever a value
/// changes so the live IMEController re-applies without waiting for
/// the next activateServer round-trip.
///
/// `@MainActor` annotation deliberately omitted — same reason as
/// `InputModeToast`: AppKit + this controller are touched only from
/// the IMK service main thread, and Swift 6 strict-concurrency would
/// otherwise force every @objc menu callback to be async (it can't).
final class SettingsWindowController {
    static let shared = SettingsWindowController()
    private var window: NSWindow?

    private init() {}

    func show() {
        if let w = window {
            w.makeKeyAndOrderFront(nil)
            NSApp.activate(ignoringOtherApps: true)
            return
        }
        let root = SettingsRootView()
        let host = NSHostingController(rootView: root)
        let win = NSWindow(contentViewController: host)
        win.title = "Inputx 设置"
        win.styleMask = [.titled, .closable, .resizable, .miniaturizable]
        win.isReleasedWhenClosed = false
        win.setContentSize(NSSize(width: 560, height: 620))
        win.center()
        self.window = win
        win.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }
}

private struct SettingsRootView: View {
    @State private var engineMode: InputxEngineMode = inputxSettings.engineMode
    @State private var japaneseEnabled: Bool = inputxSettings.japaneseEnabled
    @State private var autoCommitPolicy: InputxAutoCommitPolicy = inputxSettings.autoCommitPolicy
    @State private var useCjkPunct: Bool = inputxSettings.useCjkPunct
    @State private var useFullWidth: Bool = inputxSettings.useFullWidth
    @State private var showRareChars: Bool = inputxSettings.showRareChars
    @State private var showResetConfirm: Bool = false
    @State private var infoBanner: String?

    var body: some View {
        Form {
            // ---- 输入方案 ----
            Section {
                Picker("引擎模式", selection: $engineMode) {
                    Text("混合（五笔 + 拼音）").tag(InputxEngineMode.mixed)
                    Text("仅五笔").tag(InputxEngineMode.wubiOnly)
                    Text("仅拼音").tag(InputxEngineMode.pinyinOnly)
                    Text("仅日语").tag(InputxEngineMode.japaneseOnly)
                }
                .onChange(of: engineMode) { newValue in
                    inputxSettings.engineMode = newValue
                    broadcastChanged()
                }

                if engineMode != .japaneseOnly {
                    Toggle("日语扩展（候选追加假名 + 共形汉字）", isOn: $japaneseEnabled)
                        .onChange(of: japaneseEnabled) { newValue in
                            inputxSettings.japaneseEnabled = newValue
                            broadcastChanged()
                        }
                }
            } header: {
                Text("输入方案")
                    .font(.headline)
            } footer: {
                Text(engineModeNote)
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }

            // ---- 输入行为 ----
            Section {
                Picker("自动上屏策略", selection: $autoCommitPolicy) {
                    Text("从不自动").tag(InputxAutoCommitPolicy.never)
                    Text("满 4 码").tag(InputxAutoCommitPolicy.onFourCodes)
                    Text("仅唯一候选").tag(InputxAutoCommitPolicy.onUniqueMatch)
                    Text("满 4 码且唯一（推荐）").tag(InputxAutoCommitPolicy.onFourCodesIfUnique)
                }
                .onChange(of: autoCommitPolicy) { newValue in
                    inputxSettings.autoCommitPolicy = newValue
                    broadcastChanged()
                }
            } header: {
                Text("输入行为")
                    .font(.headline)
            } footer: {
                Text("控制 Inputx 何时不等用户选择就直接 commit 顶部候选。")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }

            // ---- 中文输入习惯 ----
            Section {
                Toggle("中文标点（，。？！…）", isOn: $useCjkPunct)
                    .onChange(of: useCjkPunct) { newValue in
                        inputxSettings.useCjkPunct = newValue
                        broadcastChanged()
                    }
                Toggle("英文 / 数字全宽", isOn: $useFullWidth)
                    .onChange(of: useFullWidth) { newValue in
                        inputxSettings.useFullWidth = newValue
                        broadcastChanged()
                    }
                Toggle("显示罕用扩展字（CJK Ext B+）", isOn: $showRareChars)
                    .onChange(of: showRareChars) { newValue in
                        inputxSettings.showRareChars = newValue
                        InputxRareChars.enabled = newValue
                        broadcastChanged()
                    }
            } header: {
                Text("中文输入习惯")
                    .font(.headline)
            }

            // ---- 学习与个性化 ----
            Section {
                Button("重置全部 L0 学习记录") {
                    showResetConfirm = true
                }
                .foregroundStyle(.red)
                Button("打开数据目录") {
                    revealL0Dir()
                }
            } header: {
                Text("学习与个性化")
                    .font(.headline)
            }

            // ---- 关于 ----
            Section {
                HStack {
                    Text("版本")
                    Spacer()
                    Text(appVersion).foregroundStyle(.secondary)
                }
                Link("源代码 (GitHub)",
                     destination: URL(string: "https://github.com/goliajp/inputx")!)
            } header: {
                Text("关于")
                    .font(.headline)
            }
        }
        .formStyle(.grouped)
        .padding(.horizontal, 12)
        .overlay(alignment: .top) {
            if let b = infoBanner {
                Text(b)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 8)
                    .background(.thinMaterial)
                    .clipShape(RoundedRectangle(cornerRadius: 10))
                    .padding(.top, 6)
                    .transition(.move(edge: .top).combined(with: .opacity))
            }
        }
        .alert("重置 L0 学习记录？", isPresented: $showResetConfirm) {
            Button("取消", role: .cancel) {}
            Button("重置", role: .destructive) {
                inputxL0Storage.reset()
                flash("学习数据已重置。")
            }
        } message: {
            Text("会删除两个引擎（五笔 + 拼音）的全部 L0 pin 和 pick counters。已经上屏的文本不受影响。")
        }
    }

    private var engineModeNote: String {
        switch engineMode {
        case .wubiOnly:     return "纯五笔输入。"
        case .pinyinOnly:   return "纯拼音输入。"
        case .japaneseOnly: return "纯日语输入：罗马字 → 平假名 / 片假名 / 共形汉字。中文引擎关闭。"
        case .mixed:        return "五笔为主，拼音兜底。\(japaneseEnabled ? "已启用日语扩展，候选末尾会附加假名 + 共形汉字。" : "")"
        }
    }

    private var appVersion: String {
        let v = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "?"
        let b = Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? "?"
        return "\(v) (build \(b))"
    }

    private func revealL0Dir() {
        let support = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first!
        let url = support.appendingPathComponent("Inputx", isDirectory: true)
        try? FileManager.default.createDirectory(at: url,
                                                  withIntermediateDirectories: true)
        NSWorkspace.shared.open(url)
    }

    private func flash(_ msg: String) {
        withAnimation { infoBanner = msg }
        DispatchQueue.main.asyncAfter(deadline: .now() + 2.5) {
            withAnimation { infoBanner = nil }
        }
    }

    private func broadcastChanged() {
        NotificationCenter.default.post(
            name: .inputxSettingsChanged,
            object: nil
        )
    }
}

extension Notification.Name {
    /// Posted by SettingsWindow / MenubarSettings whenever any inputx
    /// setting changes. Subscribed by `InputxController` instances so
    /// the live session re-applies without waiting for the next
    /// `activateServer` boundary.
    static let inputxSettingsChanged = Notification.Name("InputxSettingsChanged")
}
