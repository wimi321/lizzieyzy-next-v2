---
id: P2-01
title: 棋谱编辑（一）后端：评论、删除、设为主分支、新对局、棋局信息、保存状态、最近打开
depends_on: [P1-44]
paths:
  - crates/document/**
  - crates/sgf/**
  - crates/app-core/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
  - docs/ARCHITECTURE.md
---

## 目标

P2 第 1 批「SGF 编辑」的后端。`document` 里已有的编辑命令接到应用层，补上新对局、棋局信息、保存状态和最近打开。界面由 Claude 另做，这张卡不碰 `apps/desktop/src` 里 `api/` 以外的文件。

Java 参考在 `../lizzieyzy-next`，只学行为。每一项你参考了哪个函数、确认到的行为是什么，写进汇报。

## 要做

1. **把已有的编辑命令接出来。** `document` 已实现，app-core、Tauri 命令和 TS API 还没有：
   - `set_comment(text)`：改当前节点的评论；空字符串表示删掉评论。
   - `delete_node(node_id)`：删除这个节点和它的子树（Java 的「删除一手」`menu.delete`）。焦点在被删的子树里时，移到它的父节点。
   - `delete_branch(node_id)`：删除这个节点所在的整条分支（Java 的「删除分支」`menu.deleteBranch`）。分支从哪里算起照 Java；节点在主线上时怎么处理也照 Java。
   - `promote_to_main(node_id)`：Java 的「设为主分支」`menu.setAsMain`。照 Java 的结果：操作之后，从根到这个节点的路径就是主线。一次撤销恢复整个操作。
   - 这四个命令都能撤销和重做，返回 `Revision`，照常发 `DocumentChanged`。
2. **新对局** `new_game(options)`：
   - 选项：棋盘宽和高、贴目、让子数、规则、双方名字和段位（可空）；
   - 让子的摆放位置照 Java；
   - 换掉当前文档，文件状态清空。其余和打开一盘新棋一样处理：分析、自动胜率曲线、死活题状态都走已有的「打开棋谱」逻辑。
   - Java 的「新建」`files.new`、「清空棋盘」`menu.clearBoard`、「设置棋盘大小」`menu.setBoard` 在界面上都用它。
3. **棋局信息** `set_game_info(info)`：
   - 可改：黑白双方的名字和段位、贴目、规则、结果、日期、赛事；
   - 能撤销和重做；根节点上其他属性原样保留，属性顺序不变，新属性加在后面；
   - 贴目写用户给的数值。写完再读，有效贴目必须等于它，野狐棋谱也一样（`KM` 的野狐换算不能把它再换一次）。
4. **保存状态：**
   - `DocumentView` 增加 `file_path`（完整路径，没有时为 null）和 `modified`（内容和上次打开、新建或保存时不同则为 true；撤销回到那个状态时变回 false）；
   - `save(path)` 成功后，`file_path`、`file_name` 改成新路径，`modified` 变回 false；
   - 野狐下载的棋谱和新对局没有路径；
   - 只移动焦点（翻手、跳转）不算修改。
5. **最近打开：**
   - 设置里保存最近打开或保存过的文件路径，新的在前，去重，条数上限照 Java；
   - 菜单打开、拖入、保存都更新它；野狐下载的棋谱不进列表；
   - `recent_files()` 读列表，`clear_recent_files()` 清空；
   - 打开列表里的文件失败时，把它从列表里去掉，照常发 `OpenFailed`。
6. **变化树的评论标记：** `VariationNodeView` 增加 `has_comment`。改评论会让 `structure_revision` 变吗？不变。所以另给一个办法让界面知道哪些节点有评论变了，办法你定，写进 `docs/ARCHITECTURE.md`。
7. TS 类型照常由 Rust 生成；`e2e/mock-backend.ts` 补上新命令（都是脚本化的 `command`，`recent_files` 是带默认值的 `query`）。

## 不做

- 摆子、起始局面、标记、试下：后面的卡。
- 覆盖文件、清空棋盘前的确认对话框：界面做。
- 界面。

## 验收标准

1. 每个新命令都有 app-core 的集成测试，覆盖：
   - 正常路径；
   - 撤销和重做后，文档和 `modified` 都回到原状；
   - 错误路径：节点不存在、删除根节点、非法的棋盘大小和让子数。
2. `set_game_info`、`new_game` 的结果保存成 SGF 再读入，对局信息一致；有一条用野狐棋谱（`fixtures/` 里已有）改贴目的测试。
3. 让子摆放有对照 Java 的测试：9、13、19 路，2 到 9 子。
4. 最近打开：去重、上限、失败时移除、重启后还在（设置文件读写）。
5. Tauri 层的测试覆盖每个新命令的参数和返回值；TS API 的测试覆盖 mock 的脚本化行为。
6. `docs/ARCHITECTURE.md` 的 4.3 和 4.7 更新。
