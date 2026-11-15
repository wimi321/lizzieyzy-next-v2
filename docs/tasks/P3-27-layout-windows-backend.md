---
id: P3-27
title: 布局与浮动窗口的后端和窗口外壳：布局设置、窗口位置、浮动棋盘窗口
depends_on: [P3-26]
paths:
  - crates/app-core/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
  - docs/ARCHITECTURE.md
---

## 目标

P3 第 8 批「布局与浮动窗口」的后端和 Tauri 外壳。界面在 P3-28（主窗口里的布局）和 P3-29（浮动棋盘）。Java 参考只学行为（`../lizzieyzy-next`，路径相对 `src/main/java/featurecat/lizzie/`）：

- 布局模式和面板：`gui/LizzieFrame.java` 3275–3545（各模式）、6317–7712（绘制）、7903–7929（面板尺寸和位置）；`Config.java` 2436–2746、3640–3817；
- 浮动棋盘：`gui/IndependentMainBoard.java`、`gui/IndependentSubBoard.java`；
- 窗口位置：`Config.java` 3005–3448（persist）、`gui/LizzieFrame.java` 907–921 和 1743–1802（`fitWindowBounds`）。

**和 Java 不同的地方**（维护者的决定见 ROADMAP 第 8 批）：

- 浮动主棋盘、主窗口保留棋盘、浮动小棋盘是三个独立的状态；菜单项只是预设（界面在 P3-29）。关掉浮动窗口不重置布局。
- 所有恢复的窗口位置（主窗口、两个浮动窗口、自定义布局里的位置）都按 `fitWindowBounds` 的规则放回屏幕里。Java 只检查主窗口。
- 「重置界面位置」立即生效，不需要重启，不动提示开关。
- 双引擎模式随对比引擎那一批做，本卡不加。

## 要做

1. **显示设置的新字段**（`DisplaySettings`，Rust 是唯一来源；生成的 TS 可选，mock 补默认值）
   - `layout_mode`：`Normal`（默认）、`Min`（精简）、`Thinking`（思考）、`MultiView`（四方图）。经典模式是 `Normal` 的一组预设，不单独存。
   - `large_sub_board`、`large_winrate_graph`：默认关，两个同时为真时拒绝（类型化错误）。
   - `board_position`：0–8，默认 4（Java 的 `BoardPositionProportion`）。
   - `floating_boards`：`{ main, keep_main_board, sub, main_on_top, sub_on_top, main_locked, sub_locked }`，默认全关。启动时按它重新打开浮动窗口。
   - `custom_layouts`：两个槽，各为 `null` 或 `CustomLayout { panels, layout_mode, large_sub_board, large_winrate_graph, board_position, main_window: WindowPlacement, floating_boards, floating_main: Option<WindowBounds>, floating_sub: Option<WindowBounds> }`。`panels` 是现有 7 个面板开关的取值（名字照现有字段）。
   - 校验：越界拒绝，旧设置文件缺字段时取默认值。
2. **窗口位置**（app-core 存，外壳读写）
   - `WindowBounds { x, y, width, height }`（逻辑像素，可为负，副屏用）；`WindowPlacement { bounds, maximized }`。
   - 设置里存主窗口、浮动主棋盘、浮动小棋盘的位置。
     - 外壳在移动、缩放结束后保存，用 `tokio::time` 去抖（约 500 ms）；退出时再存一次。
     - 不在快速连续的事件里反复写盘。
   - 纯函数 `fit_window_bounds(bounds, work_areas, minimum)`，照 Java：
     - 选和窗口重叠面积最大的显示器工作区；面积相同选中心最近的；都不重叠选主显示器。
     - 先缩到工作区以内（不小于最小尺寸），再平移进去。
     - 单元测试加 proptest：结果总在某个工作区内。
   - 启动时：主窗口用保存的位置（经过 `fit_window_bounds`），没有就照现在居中；保存时是最大化就最大化。
   - 主窗口最小尺寸改成 520×400（Java 的值），否则精简模式和浮动模式的窄窗口做不出来。
   - 命令 `fit_main_window(width, height)`：窗口没最大化时改成这个大小，保持左上角，经过 `fit_window_bounds`；最大化时不动。给 P3-28 切换布局用。
   - 命令 `reset_window_positions()`：
     - 清掉保存的三个窗口位置，`board_position` 回 4；
     - 已打开的窗口立刻回到默认位置和大小（主窗口 1280×820 居中；浮动窗口见下）；
     - 不关浮动窗口，不改模式和面板开关，不动提示开关。
3. **浮动棋盘窗口**（外壳）
   - 两个窗口，label `float-main`、`float-sub`，加载同一个前端，URL 带 `?window=float-main` / `?window=float-sub`（开发模式下同一个开发服务器）。界面在 P3-29，本卡只保证窗口打开后前端能读到 `window` 参数。
   - 无边框（Java 也是），可以置顶，有任务栏图标与否照平台默认。
   - 默认位置：浮动主棋盘 600×600，放在主显示器右侧、垂直居中；浮动小棋盘 300×300，主显示器左侧、垂直居中。保存过的位置经过 `fit_window_bounds`。
   - 命令：
     - `open_floating_board(kind)`、`close_floating_board(kind)`，`kind` 为 `Main` / `Sub`；
     - `set_floating_board_options(kind, on_top, locked)`；
     - `set_floating_layout(main, keep_main_board, sub)`：一次设好三个状态，开关对应窗口，供菜单预设用。
   - 用户用系统方式关掉浮动窗口时，状态同步改为关，并发出事件。
   - 新事件 `FloatingBoardsChanged(FloatingBoardsView)`：三个状态加两个窗口的选项。显示设置变了照现有方式通知；所有窗口都能订阅现有事件，订阅方式不变。
   - capability：新增一份 `floating.json`，只给 `float-main`、`float-sub`。
     - 只放浮动棋盘需要的命令：读取设置和文档、订阅事件、落子、停一手、翻手（`step`、`go_to`）、变化预览、选点限制的圈选和清除、暂停/继续分析、自己的关闭和选项命令。
     - 窗口拖动和缩放需要的 core 权限：`start_dragging`、`start_resize_dragging`、`set_size`。
     - 不给文件、网络、更新、引擎配置之类的权限。capability 的测试照现有 `main.json` 的方式写。
4. **TS 接口**：新命令进 `src/api/`，类型由 Rust 导出；mock 里是脚本化的 `command`，新事件可以用 `pushEvents` 推。

## 不做

- 界面（P3-28、P3-29）。
- 双引擎模式（随对比引擎）。
- 拖动分隔条和三个拖动比例（Java 的 `leftoverLeftShare` 等）。

## 验收标准

1. 设置：每个新字段的默认值、越界拒绝、旧设置文件缺字段、两个放大同时为真被拒，各有测试。
2. `fit_window_bounds`：多显示器、负坐标、比工作区大、完全在屏幕外、平局选中心最近，单元测试加 proptest。
3. 外壳：打开、关闭、系统方式关闭后状态和事件、重复打开不出第二个窗口、`set_floating_layout` 的各种组合、`reset_window_positions`、`fit_main_window` 在最大化时不动，用 `tauri` 的 `test` 特性测（不打开真实窗口）；去抖用假时钟测。
4. capability：浮动窗口能调用的命令就是上面列的这些，有测试。
5. `docs/ARCHITECTURE.md` 写明窗口位置的存储和屏幕检查规则、浮动窗口的 capability。
6. `scripts/verify.sh --e2e` 通过，`check_budgets.py` 通过。
