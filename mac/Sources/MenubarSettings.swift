import Cocoa
import InputxKit

/// Menubar (NSStatusItem) settings UI — the Mac counterpart to the iOS
/// container app's Settings screen. Lives in the system menu bar so users
/// can flip toggles without leaving their current app.
///
/// Settings persist to `UserDefaults` via `InputxSettings`. The settings
/// take effect on the *next* `activateServer` call (i.e., the next time the
/// user clicks into a text field) — IMKit doesn't expose a "broadcast
/// settings changed" hook, and per-IMKInputController instances re-read
/// settings on `activateServer`.
final class MenubarSettings {
    private let statusItem: NSStatusItem
    private let menu = NSMenu(title: "Inputx")

    init() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        if let button = statusItem.button {
            // U+5165 "入" — visual mnemonic for CJK input. Flipped to "A"
            // while the controller is in EN mode (see `handleModeChanged`).
            button.title = "入"
            button.font = NSFont.systemFont(ofSize: 14, weight: .semibold)
        }
        statusItem.menu = menu
        rebuildMenu()
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleInputModeChanged(_:)),
            name: .inputxInputModeChanged,
            object: nil
        )
    }

    deinit {
        NotificationCenter.default.removeObserver(self)
    }

    /// Update the status-item title to reflect the current CJK / EN mode.
    /// Posted by `InputxController.toggleInputMode`.
    @objc private func handleInputModeChanged(_ note: Notification) {
        guard let raw = note.userInfo?["mode"] as? UInt8,
              let mode = InputxInputMode(rawValue: raw)
        else { return }
        if let button = statusItem.button {
            button.title = (mode == .cjk) ? "入" : "A"
        }
    }

    private func rebuildMenu() {
        menu.removeAllItems()

        // Engine mode picker
        let modeHeader = NSMenuItem(title: "输入方案", action: nil, keyEquivalent: "")
        modeHeader.isEnabled = false
        menu.addItem(modeHeader)
        addModeItem("混合（五笔为主，拼音兜底）", mode: .mixed)
        addModeItem("仅五笔", mode: .wubiOnly)
        addModeItem("仅拼音", mode: .pinyinOnly)
        menu.addItem(.separator())

        // Auto-commit policy
        let policyHeader = NSMenuItem(title: "自动上屏", action: nil, keyEquivalent: "")
        policyHeader.isEnabled = false
        menu.addItem(policyHeader)
        addPolicyItem("永不", policy: .never)
        addPolicyItem("满 4 码即提交", policy: .onFourCodes)
        addPolicyItem("唯一候选时提交", policy: .onUniqueMatch)
        addPolicyItem("满 4 码且唯一时提交（推荐）", policy: .onFourCodesIfUnique)
        menu.addItem(.separator())

        // Toggles
        addToggle("中文标点（，。？！…）",
                  isOn: inputxSettings.useCjkPunct,
                  selector: #selector(toggleCjkPunct))
        addToggle("英文数字全角",
                  isOn: inputxSettings.useFullWidth,
                  selector: #selector(toggleFullWidth))
        addToggle("显示生僻字（Plane-2+ 需安装 InputxCJKExtended 字体）",
                  isOn: inputxSettings.showRareChars,
                  selector: #selector(toggleRareChars))
        menu.addItem(.separator())

        // L0 user-learning actions
        let l0Header = NSMenuItem(title: "学习记录 (L0)", action: nil, keyEquivalent: "")
        l0Header.isEnabled = false
        menu.addItem(l0Header)
        menu.addItem(menuItem("打开数据目录…", selector: #selector(revealL0Dir)))
        menu.addItem(menuItem("重置（清空所有学习）", selector: #selector(resetL0)))
        menu.addItem(.separator())

        // About / quit
        menu.addItem(menuItem("关于 Inputx", selector: #selector(showAbout)))
        menu.addItem(menuItem("退出 Inputx", selector: #selector(quit)))
    }

    // MARK: - Item builders --------------------------------------------------

    private func addModeItem(_ title: String, mode: InputxEngineMode) {
        let item = NSMenuItem(title: title,
                              action: #selector(pickMode(_:)),
                              keyEquivalent: "")
        item.target = self
        item.tag = Int(mode.rawValue)
        item.state = (inputxSettings.engineMode == mode) ? .on : .off
        menu.addItem(item)
    }

    private func addPolicyItem(_ title: String, policy: InputxAutoCommitPolicy) {
        let item = NSMenuItem(title: title,
                              action: #selector(pickPolicy(_:)),
                              keyEquivalent: "")
        item.target = self
        item.tag = Int(policy.rawValue)
        item.state = (inputxSettings.autoCommitPolicy == policy) ? .on : .off
        menu.addItem(item)
    }

    private func addToggle(_ title: String, isOn: Bool, selector: Selector) {
        let item = NSMenuItem(title: title, action: selector, keyEquivalent: "")
        item.target = self
        item.state = isOn ? .on : .off
        menu.addItem(item)
    }

    private func menuItem(_ title: String, selector: Selector) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: selector, keyEquivalent: "")
        item.target = self
        return item
    }

    // MARK: - Actions --------------------------------------------------------

    @objc private func pickMode(_ sender: NSMenuItem) {
        guard let mode = InputxEngineMode(rawValue: UInt8(sender.tag)) else { return }
        inputxSettings.engineMode = mode
        rebuildMenu()
    }

    @objc private func pickPolicy(_ sender: NSMenuItem) {
        guard let p = InputxAutoCommitPolicy(rawValue: UInt32(sender.tag)) else { return }
        inputxSettings.autoCommitPolicy = p
        rebuildMenu()
    }

    @objc private func toggleCjkPunct() {
        inputxSettings.useCjkPunct.toggle()
        rebuildMenu()
    }

    @objc private func toggleFullWidth() {
        inputxSettings.useFullWidth.toggle()
        rebuildMenu()
    }

    @objc private func toggleRareChars() {
        inputxSettings.showRareChars.toggle()
        InputxRareChars.enabled = inputxSettings.showRareChars
        rebuildMenu()
    }

    @objc private func revealL0Dir() {
        let support = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first!
        let url = support.appendingPathComponent("Inputx", isDirectory: true)
        try? FileManager.default.createDirectory(at: url,
                                                  withIntermediateDirectories: true)
        NSWorkspace.shared.open(url)
    }

    @objc private func resetL0() {
        let alert = NSAlert()
        alert.messageText = "重置 L0 学习记录？"
        alert.informativeText = "将删除所有自动学习的固定候选。已经上屏的文本不受影响。"
        alert.alertStyle = .warning
        alert.addButton(withTitle: "重置")
        alert.addButton(withTitle: "取消")
        if alert.runModal() == .alertFirstButtonReturn {
            inputxL0Storage.reset()
        }
    }

    @objc private func showAbout() {
        NSApp.activate(ignoringOtherApps: true)
        let alert = NSAlert()
        alert.messageText = "Inputx 输入法"
        let version = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "?"
        alert.informativeText = """
            版本 \(version)
            © 2026 GOLIA K.K.
            MIT OR Apache-2.0

            隐私优先的中文输入法，五笔为主，拼音兜底。
            完全本地运行，零联网。

            源代码：https://github.com/goliajp/inputx
            """
        alert.runModal()
    }

    @objc private func quit() {
        NSApp.terminate(nil)
    }
}
