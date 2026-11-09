---
id: P1-44
title: 主窗口严格还原的后端补充：拖入棋谱、标题里的文件名、引擎显示名
depends_on: [P1-40]
paths:
  - crates/app-core/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
  - scripts/packaging/**
  - resources/licenses/**
  - docs/ARCHITECTURE.md
---

## 目标

P1-40 照 LizzieYzy Next 还原了窗口框架，还有三处要后端提供数据：
1. 空棋盘提示写着「把 SGF 拖进来」。Java 版把棋谱拖进窗口就会打开，新版还不行。
2. Java 的窗口标题最后是文件名（「… - game.sgf」），新版拿不到。
3. 菜单栏、标题、状态栏里的引擎名，Java 显示「Transformer B11 · 2026-09-25」，新版只能显示模型文件名「kata1-tf3-b11c768」。

另外，P1-41 会在界面里用 Java 自带的 Open Sans Semibold 字体，安装包要带上它的许可证。

分工：后端由 Codex 做。标题、状态栏、菜单栏怎么用这些数据，由 Claude 在界面里接。Codex 在前端只改 `src/api/`、mock-backend 和 fixtures。

## 要做

1. **拖入棋谱**（参照 `gui/LizzieFrame.java` 的 `importDroppedKifuFiles`、`finishDroppedKifuImport`）
   - 在 Rust 侧处理窗口的拖放事件（Tauri 的 `WindowEvent::DragDrop`），不新增 IPC 权限。
   - 拖进一个文件，就照「文件 → 打开」打开它，走 `open_path` 同一条路径，所有规则一样。
   - 拖进多个文件：Java 会开批量分析，新版还没有这个功能，先只打开第一个能打开的文件。
   - 只接受「文件 → 打开」能打开的扩展名（现在是 `.sgf`，不分大小写），其他文件不理。
   - 打开失败时发 `AppEvent::OpenFailed { path, reason }`，界面以后据此提示。
2. **标题里的文件名**（参照 `LizzieFrame.fileNameTitle`）
   - `DocumentView` 加 `file_name: string | null`：从文件打开或保存到文件之后，是这个文件的文件名，不带路径；另存为之后换成新文件名。
   - 不是来自文件的棋局为 null：野狐棋谱、棋盘同步、死活题等。
3. **引擎显示名**（参照 `util/KataGoAutoSetupHelper.java` 的 `toWeightDisplayName` 及其测试，`analysis/Leelaz.java` 的 `deriveDisplayName`）
   - 新增查询命令 `engineDisplayName(profile: EngineProfile) -> EngineDisplayName`：按 Java 的规则，从认出的目录模型（`canonical_model_name`）或模型文件名得出显示名。
   - Java 里有两个名字随界面语言变：10B 轻量版、10B 均衡版。这两种返回目录里的模型 id，由界面翻译。其余返回文字，例如 B11 旗舰返回「Transformer B11 · 2026-09-25」，日期取目录的 `publishedAt`。
   - 类型：`{ type: "Text", data: string } | { type: "Catalog", data: string }`，data 分别是文字和目录 id（如 `"b10-balanced"`）。
4. **Open Sans 的许可证**：加 `resources/licenses/OpenSans-LICENSE.txt`（Apache License 2.0 全文），安装包照其他许可证的做法带上它，打包的测试一并更新。

## 不做

- 界面：标题、状态栏、菜单栏、失败提示怎么显示，由 Claude 接。
- 批量分析。

## 验收标准

1. Rust 测试：
   - 拖放选文件：单个、多个、扩展名大小写、不支持的文件；
   - `file_name` 在打开、另存为、野狐导入之后的值；
   - `engineDisplayName` 覆盖 Java 测试里的样例，以及目录里的三个模型。
2. mock-backend 支持 `engineDisplayName`；老的 fixture 缺 `file_name` 时按 null。
3. `scripts/verify.sh` 以 `verify: ALL PASSED` 结束；scope ok。
