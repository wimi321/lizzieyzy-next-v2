---
id: P1-27
title: 菜单栏：照 LizzieYzy Next 一比一还原
depends_on: [P1-26]
paths:
  - apps/desktop/src/**
  - apps/desktop/e2e/**
  - apps/desktop/src-tauri/capabilities/main.json
  - docs/tasks/P1-27-menu-bar.md
  - docs/ROADMAP.md
  - docs/ARCHITECTURE.md
---

## 目标

维护者 2026-10-30：「还是一:一还原原生菜单吧，不然大家用着不习惯」。主窗口恢复 LizzieYzy Next 的菜单栏，老用户在原来的位置找到原来的菜单项。界面由 Claude 做。

## 要做

- 位置：菜单条画在窗口里、工具栏上方。Java 版在 Windows 和 macOS 上用的都是窗口内自绘菜单条（`MenuPresentationMode.CUSTOM_STRIP`），不是系统菜单栏。
- 内容：以参考提交 993e288 的 `Menu.java` 默认显示的菜单为准，菜单、子菜单、分隔线、顺序和文字逐项一致。
  - 文字取自 `locales/legacy` 里的 Java 译文。
  - 末尾带快捷键的文字，快捷键靠右显示，规则同 `MenuShortcutHint`。
- 不放的内容：
  - `docs/inventory/FEATURES.md` 定为「砍掉」的项：外观主题、主题、界面外观、打开弈客网页版、弈客大厅、强制退出。
  - Java 默认隐藏的菜单：快速启动、第二引擎菜单。
  - 菜单条上的 AI 陪练 / AI 解说按钮（P3）。
- 已有的功能接上，其余灰显：
  - 文件：打开棋谱、保存、另存为、退出。新版还不记文件路径，所以「保存」也会弹保存对话框。
  - 显示：坐标、Kata 形势（关闭 / 显示在大棋盘上）。新版固定显示的面板和悬停变化图显示为勾选、不可改。
  - 棋局：让 AI 落子最佳一手、停一手。
  - 分析：开始/停止分析、自动分析、整盘精析（打开自动分析面板）、停止自动分析、棋力水平评估。
  - 编辑：返回主分支、跳转到最前 / 最后 / 左分支 / 右分支。
  - 同步：野狐棋谱。帮助：关于。设置：引擎。
  - 引擎菜单：标题是当前权重名。引擎启动失败时，「重启当前引擎」可用。
- 快捷键照 Java 的 `Input.java`：
  - ↑↓ 前后一手，Ctrl/Cmd+↑↓ 前后十手，←→ 切换分支，Home / End 到首尾。
  - 空格开始/停止分析，A 自动分析，O 打开，S 另存为，Ctrl/Cmd+S 保存，P 停一手，逗号落最佳一手，C 坐标，B 返回主分支，Alt+X 引擎设置。
  - 在输入框、对话框和菜单里按键不触发这些快捷键。
- 鼠标滚轮在棋盘上前后翻手。
- 菜单的键盘操作：F10 进入菜单条，方向键移动，回车执行，Esc 逐级关闭。

## 验收标准

1. Vitest：
   - 菜单结构：顺序、id 唯一、分隔线位置；每个文字与 Java 中英文译文一致。
   - 快捷键提示的拆分沿用 Java 的测试用例。
   - 分支导航与 Java 的 `previousBranch` / `nextBranch` / `moveToMainTrunk` 一致。
   - 菜单的鼠标和键盘操作；快捷键在输入框和对话框里不触发。
2. Playwright：菜单顺序、悬停切换和子菜单，从菜单打开野狐棋谱、自动分析、关于；空格切换分析。截图基线在 Linux 上重新生成。
3. `scripts/verify.sh --e2e` 通过，`check_scope.py` 无越界。
4. 维护者在 macOS 上对照 LizzieYzy Next 确认菜单一致、快捷键顺手。
