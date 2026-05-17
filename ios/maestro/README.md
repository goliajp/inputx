# Inputx — Maestro 测试流程

用 [Maestro](https://maestro.mobile.dev) 在 iOS Simulator 上跑 UI 自动化测试。

## 环境

- 默认目标 sim: **`sim-inputx`** (iOS 26.4)，UDID 由 `xcrun simctl list devices "sim-inputx"` 查
- Maestro: `/Users/doracawl/.maestro/bin/maestro` (v2.2.0+)
- 构建脚本: `../build_sim.sh` (env override 通过 `IOS_CONFIG=Debug|Release` + `SIMULATOR_DEVICE`)

## 一次性 setup（每个新 sim 都要做一次）

1. **创建 sim**（如果还没建过）：
   ```sh
   xcrun simctl create "sim-inputx" \
     com.apple.CoreSimulator.SimDeviceType.iPhone-17 \
     com.apple.CoreSimulator.SimRuntime.iOS-26-4
   ```
2. **首次启动 + 装 Inputx**：
   ```sh
   cd ios && ./build_sim.sh
   ```
3. **手动添加 Inputx 键盘**（Maestro 不能跨 system prompts 自动化这步）：
   - Simulator → Settings → General → Keyboards → Keyboards → Add New Keyboard → Inputx
   - 开启 **Allow Full Access**

完成后，后续 Maestro flow 即可使用 Inputx 键盘。

## 运行测试

最常用：
```sh
./maestro_test.sh 01-smoke           # 跑单个 flow
./maestro_test.sh                    # 跑全部 flows（默认 flows/ 目录）
./maestro_test.sh --no-build 01-smoke   # 跳过 build/install，仅跑 maestro
```

或者直接调 Maestro：
```sh
~/.maestro/bin/maestro test ios/maestro/flows/01-smoke.yaml
```

## 现有 Flows

| 文件 | 范围 | 备注 |
|---|---|---|
| `01-smoke.yaml` | 启动 InputxApp，截图 | 最基础冒烟，确认 install + launch 正常 |
| `02-keyboard-typing.yaml` | Notes 里测打字 | 需要 Inputx 已设为活跃键盘 |
| `03-inputx-settings.yaml` | App 内 SettingsView | UI binding 回归 |

## 加新 flow

1. 在 `flows/` 下新建 `NN-名字.yaml`（数字前缀用于排序）
2. 头部 frontmatter：
   ```yaml
   appId: jp.golia.inputx       # 或 com.apple.mobilenotes 等
   name: 简短名
   tags:
     - 分类标签
   ---
   ```
3. 流程命令参考 [Maestro YAML reference](https://maestro.mobile.dev/api-reference/commands)

## Keyboard extension 测试限制

Maestro / XCUITest 对 iOS 第三方键盘 extension 的覆盖有限：

- **系统级 prompts**（"Allow Full Access" 等）— 通常需要手动一次性处理
- **键盘切换**（globe key）— 系统行为，maestro 不直接控制；保持 Inputx 是默认键盘最简单
- **Accessibility tree** — Inputx 的 `KeyButton` 都设了 `accessibilityLabel`，理论上 Maestro 能 tap 到。实际可达性需要 case-by-case 验证

## 常用 debug

- 当前 sim 状态：`xcrun simctl list devices booted`
- Maestro Studio（GUI 录制工具）：`maestro studio`
- 单步调试 flow：`maestro test --continuous flows/xxx.yaml` 会监听文件修改重跑
