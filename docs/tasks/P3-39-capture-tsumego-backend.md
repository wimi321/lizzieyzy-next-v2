---
id: P3-39
title: 屏幕抓取死活题的后端：截屏、识别棋子、抓取会话、生成新棋局
depends_on: [P3-37]
paths:
  - Cargo.toml
  - scripts/architecture.json
  - crates/board-capture/**
  - crates/app-core/**
  - fixtures/board-capture/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
  - docs/ARCHITECTURE.md
---

## 目标

P3 第 11 批「屏幕抓取死活题」（Java 的 分析 → 抓取死活题(Shift+T)）的后端。用户把书、网页或别的软件里的死活题截下来，识别成棋子，在新棋局里摆好，交给已有的死活题功能（P1-32）补围墙和劫材。界面在 P3-40。

Java 参考只学行为（`../lizzieyzy-next`，路径相对 `src/main/java/featurecat/lizzie/`）：

- `analysis/CaptureTsumeGo.java`：启动辅助程序、读它的输出、摆棋子（`processStoneData` 180–223）。
- `gui/CaptureTsumeGoFrame.java`、`gui/CaptureTsumeGoSettings.java`：说明窗口和五个阈值。
- `Config.java` 1261–1265、2063–2067：阈值的键和默认值。
- 截屏和识别在打包的辅助程序 `src/main/resources/assets/captureTsumeGo/CaptureTsumeGo1.2.jar` 里，源码不在仓库。用 `javap -c` 看 `captureTsumeGo.CaptureTsumego` 的 `drawSelectArea`、`recognizeBoard`、`getColorPercent` 和 `captureTsumeGo.CaptureGo.main`。

**和 Java 不同的地方**（决定见 ROADMAP 第 11 批）：

- 不用辅助 jar，也不需要 Java 运行时：截屏在 Tauri 外壳里用 Rust 库完成，识别是纯 Rust 函数。阈值、默认值和判定规则同 Java。
- 先定格再选：先截下整块显示器，用户在全屏窗口里对着这张静止的截图点三下。Java 是半透明遮罩，每移动一次鼠标就重新截屏，遮罩自己的蓝线和预览点会混进被采样的像素。
- 截主窗口所在的显示器；Java 只截主显示器。高分屏按截图的实际像素换算。
- 结果放进一盘新棋局（摆子根节点，黑先）。Java 不询问就清空当前棋局。
- 确认前可以逐点修正，可以选棋盘大小；Java 都没有。
- macOS 检查「屏幕录制」权限，缺少时由界面用当前语言说明，并能打开系统设置。Java 每次启动都弹一段写死的中文。
- 阈值有范围检查；Java 不检查。
- 修掉 Java 的问题：高度错用宽度（`reopen(bw, bw)`）、失败后主窗口一直最小化、可以同时开多个抓取。

## 要做

1. **识别**（新的纯 crate `crates/board-capture`：同步、无 I/O、无 tokio；在 `scripts/architecture.json` 登记为纯 crate，只依赖 go-core，也可以不依赖任何 crate）
   - 输入：RGBA 图像（宽、高、行优先像素）、网格、阈值。
   - **拟合网格**（同 Java `drawSelectArea`）：
     - 三个点都是图像像素坐标。点 1 是题目的角上那个交叉点，点 2 是斜对的第一个交叉点，点 3 是题目范围的对角。
     - 格宽、格高分别是点 2 与点 1 在 x、y 上的距离；任何一个为 0 返回错误。
     - 点 2 在点 1 的哪个方向决定角：右下 → 左上角，左下 → 右上角，右上 → 左下角，左上 → 右下角。
     - 点 3 定范围：列数 = round(宽 / 格宽) + 1，实际格宽 = 宽 / round(宽 / 格宽)（Java 的 `curGap`），行同理。点 3 和点 1 在同一列或同一行时只有一列或一行。
   - **判定**（照 Java `recognizeBoard` / `getColorPercent`，常数照抄）：
     - 以交叉点为中心取一格（实际格宽 × 实际格高）。超出图像时移回图像内。
     - 只数「灰色」像素：三个通道两两之差都小于灰度偏色。
     - 其中三个通道都 ≤ 黑偏色的算黑；都 ≥ 255 − 白偏色的算白；都 ≥ 225 的算纯白；都 ≥ 190 的算近白。各自按占整格面积的百分比（整数）计。
     - 白占比够时检查 trueWhite：在格子 h/4 的那一行，从 0 到 w·2/6，有任何通道 < 225 的像素即为真。
     - 黑占比 ≥ 黑最低占比 → 黑；否则白占比 ≥ 白最低占比，且（trueWhite，或 近白 − 纯白 ≥ 10）→ 白；否则空。
   - **放置**：`place(cells, corner, board_size)` 把识别结果按角放到棋盘上，点 1 对着棋盘的那个角。识别范围大于棋盘时返回类型化错误。
   - 输出 `rows × columns` 的 空 / 黑 / 白，以及拟合后的网格（原点、实际格宽格高、列数、行数、角），供界面画预览。
2. **设置**
   - `Settings.capture: CaptureSettings { black_offset: 120, black_percent: 25, white_offset: 120, white_percent: 15, gray_offset: 80 }`。旧文件缺字段时用默认值，版本仍为 1。
   - 范围：三个偏色 0..=255，两个占比 1..=100。
   - `capture_settings()` / `set_capture_settings(settings)`：先原子保存再生效，失败保留旧值。
3. **抓取会话**（app-core 持有状态；截屏由外壳交给它）
   - `begin_capture(image)`：取代任何旧会话，返回 `CaptureSession { id, width, height, scale_factor }`。图像只留在内存里，不写盘。
   - `recognize_capture(id, points)`：按当前阈值拟合并识别，返回网格和 cells。
   - `finish_capture(id, CaptureResult { corner, cells, board_size })`：
     - `cells` 是界面修正后的结果，尺寸必须和 `corner` 一起能放进 `board_size × board_size`（2..=25）。
     - 建一盘新棋局：摆子根节点（AB/AW），PL[B]，规则和贴目按当前引擎设置的默认值（照 `new_game` 和 `generate_tsumego` 建根节点的写法）。当前棋局像打开别的棋谱一样被替换；有没有未保存的改动由界面在开始前问。
     - 结束会话，释放图像。
   - `cancel_capture(id)`：结束会话，释放图像。
   - 类型化错误：没有会话、会话已过期、网格无法拟合、放不下、cells 尺寸不对。
4. **外壳**（`apps/desktop/src-tauri`）
   - **截屏库**：本卡允许新增**一个**截屏依赖（例如 `xcap`）。先确认许可证和 GPL-3.0 兼容、新增的传递依赖尽量少，汇报里写理由并贴出 `cargo tree -i <crate>`。三个平台在 CI 上都要能编译；如果 Linux 需要 CI 镜像上没有的系统包，停下来在汇报里说明，不要改 `.github/`。
   - **macOS 权限**（不写 `unsafe`；锁文件里已有 `core-graphics`，先看它的 `access::ScreenCaptureAccess` 能否直接用）：
     - `capture_permission() -> Granted | Missing | NotNeeded`；
     - `request_capture_permission()`：弹出系统询问；
     - `open_capture_permission_settings()`：打开固定的系统设置地址（隐私与安全性 → 屏幕录制）。
   - **`start_screen_capture()`**：
     - macOS 缺权限时直接返回 `PermissionMissing`，什么窗口都不动。
     - 选主窗口中心所在的显示器，隐藏主窗口和正在显示的浮动棋盘，确认它们已隐藏（有上限的短暂等待）后，按该显示器的实际像素截屏。
     - 交给 app-core `begin_capture`，再打开抓取窗口 `capture`（页面 `?window=capture`）：无边框、置顶、不可调整大小，正好覆盖那块显示器。
     - 返回会话信息和显示器的逻辑尺寸。
     - 同一时间只允许一个抓取：已在进行时把抓取窗口提到前面，返回 `Busy`。
   - `capture_image(id)`：用原始二进制 IPC 返回截图（照 P3-01 的 PNG 写法，可以是 PNG 或其他浏览器能直接显示的格式）。
   - `finish_capture` / `cancel_capture` 之后关闭抓取窗口，重新显示主窗口和原来显示的浮动棋盘，聚焦主窗口。
   - **截屏或开窗任何一步失败**：恢复所有窗口，返回原因。Linux 支持 X11；Wayland 下库截不了屏时返回类型化的 `Unsupported`。
   - **事件** `CaptureEnded { outcome: Finished | Cancelled | Failed { reason } }`：主窗口据此打开死活题对话框或显示原因。
   - **权限文件**：
     - 新建 `capabilities/capture.json`，抓取窗口只拿到 `capture_image`、`recognize_capture`、`finish_capture`、`cancel_capture`、`capture_settings`；
     - 主窗口拿到 `start_screen_capture`、权限三个命令、`capture_settings`、`set_capture_settings`、`cancel_capture`；
     - 浮动窗口都不授权。照现有 capability 测试补上。
   - 命令放进新文件 `src/commands/capture.rs`。
5. **类型和 mock**
   - 生成 TS；新设置字段在 TS 里是可选的，mock 补默认值。
   - mock：`captureSettings` 和 `capturePermission` 是查询（不写脚本时回答默认阈值和 `NotNeeded`），其余命令是脚本化的 `command`，`CaptureEnded` 可以用 `pushEvents` 推。
   - ARCHITECTURE 同步。

## 不做

- 界面：说明窗口、抓取窗口的交互、修正、权限说明（P3-40）。
- 自动找棋盘、识别轮走方或坐标文字（Java 也没有）。
- 跨多块显示器的选区。

## 验收标准

- `board-capture` 的纯函数测试：
  - 拟合：四个方向、整数格数、零格宽报错、只有一行一列；
  - 判定：每一条规则各有构造的像素例子，包括黑白都够时黑优先、白纸上的白子靠 trueWhite / 近白区分、灰度偏色过滤掉彩色；
  - 放置：四个角、放不下报错；
  - 用代码生成的合成棋盘图（木色底和白纸底、网格线、黑白子、带数字的子）识别结果和布局一致；proptest 随机布局。
- 金样本：`fixtures/board-capture/` 里放几张新版自己渲染的棋盘截图（从仓库的 e2e 基线裁出一角即可，不用外部图片），配上期望结果。
- app-core：
  - 注入图像后识别；修正后的 cells 建出新棋局（根节点摆子、PL[B]、尺寸、默认贴目）；
  - 过期会话、取消后释放、放不下、cells 尺寸不对；
  - 阈值的保存、范围检查和旧文件默认值。
- 外壳：
  - 选显示器、逻辑坐标和像素的换算写成纯函数并单元测试；
  - 新命令只授权给上面列出的窗口；
  - 真实截屏不进 CI。
- 生成的 TS 类型和 mock 同步。
- `scripts/verify.sh --e2e` 通过；`check_budgets.py` 通过。
