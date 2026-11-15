---
id: P3-24
title: 存档与读档的后端，以及自动播放、涂鸦和控制面板的设置
depends_on: [P3-22]
paths:
  - crates/app-core/**
  - crates/document/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
  - fixtures/**
  - docs/ARCHITECTURE.md
---

## 目标

P3 第 7 批「研究工具」的另一半后端。界面在 P3-26。

Java 的「文件 → 存档与读档」和顶栏「存档」（`gui/LizzieFrame.java` 15044–15301、`Lizzie.java` 1476、`LizzieFrame.java` 1566–1614，路径相对 Java 仓库的 `src/main/java/featurecat/lizzie/`）提供三样东西：

- 多个存档槽，每个有缩略棋盘；
- 自动存档；
- 启动时恢复上次的棋局。

新版保留这三样，但修掉 Java 的问题：

- 存到应用数据目录，不放工作目录下的 `save/`；
- 缩略图不存图片，由界面按局面画；
- 不在后台线程截图写文件。

## 要做

1. **存档槽**（应用数据目录下的 `saves/`，每个槽一个 SGF 加一份元数据）
   - 元数据：
     - id；
     - 名称；
     - 保存时间（Unix 毫秒）；
     - 存档时所在节点的路径（从根到该节点每一步选第几个子节点），以及该节点的手数；
     - 该节点局面的快照，供界面画缩略棋盘：宽、高、每个点的颜色。
   - 命令：
     - `save_slots()`：按保存时间倒序列出所有槽；自动存档单独一项，标明是自动存档。
     - `save_to_new_slot(name)`：名称为空时用棋谱的默认文件名。
     - `overwrite_slot(id)`。
     - `rename_slot(id, name)`。
     - `load_slot(id)`：打开该槽的棋谱并跳到存档时的节点。打开的流程和打开文件一样，包括尚未保存的改动怎么处理、试下中怎么处理。节点路径失效时退回到同手数的主线节点，再不行就到末手。
     - `delete_slot(id)`。
     - `delete_all_slots()`：不删自动存档。
   - 名称：去掉首尾空白，限制长度，不允许控制字符。
   - 槽数上限 100，超出返回类型化错误。
   - 文件损坏的槽在列表里标为损坏，可以删除；读不了不能让列表或启动失败。
   - 写文件用先写临时文件再改名的方式，不留下半个文件。
2. **自动存档**
   - 设置：`autosave`（默认开）、`restore_last_game`（默认关）。
   - 时机：
     - 退出程序时写一次；
     - 运行时每 60 秒检查一次，只有棋谱自上次自动存档后改过时才写。
     - 人机对局、引擎对局、陪练、批量分析进行中不写。
     - 用 `tokio::time`，测试用暂停的时间推进，不等真实时间。
   - 自动存档只有一个槽，每次覆盖。
   - `restore_last_game` 开着时，启动后打开自动存档的棋谱并跳到当时的节点。失败只记日志，不影响启动，也不弹错。
   - 命令：`set_autosave(enabled)`、`set_restore_last_game(enabled)`，值也出现在设置里。
3. **界面设置**（放进现有的 `DisplaySettings`，经 `set_display_settings` 保存）
   - `autoplay_interval_ms`：自动播放的间隔，100–60000，默认 2000。
   - `painting_color`：`Blue` / `Green` / `Red`，默认 `Red`。
   - `control_panel_bounds`：独立棋局控制面板的位置和大小，可为空。
   - 都要校验范围，越界返回类型化错误。
4. 新的设置和视图字段在生成的 TS 里可选，mock 补默认值；新命令在 mock 里是脚本化的 `command`。
5. 诊断包不收集存档内容，只记槽数。

## 不做

- 界面（P3-26）。
- 导入 Java 的 `save/` 目录。
- 把涂鸦存进存档或 SGF（涂鸦不保存，这一点同 Java）。

## 验收标准

1. 存档槽的每个命令各有测试：
   - 列表顺序；
   - 默认名称；
   - 覆盖；
   - 重命名的校验；
   - 读取后跳到原节点，路径失效时的退路；
   - 删除；
   - 全部删除时保留自动存档；
   - 上限；
   - 损坏的槽。
2. 自动存档：
   - 退出时写；
   - 只在改动后写；
   - 对局和批量分析中不写；
   - 启动恢复开或关；
   - 恢复失败不影响启动。
   各有测试，时间用暂停推进。
3. 写到一半失败不留下半个文件，有测试。
4. 新的界面设置越界被拒绝，有测试。
5. `scripts/verify.sh --e2e` 通过；`docs/ARCHITECTURE.md` 写明存档的位置、格式和自动存档的时机。
