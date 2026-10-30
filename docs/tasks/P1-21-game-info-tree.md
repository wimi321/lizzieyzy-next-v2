---
id: P1-21
title: app-core：对局信息、变化树与变化预览
depends_on: [P1-13]
paths:
  - crates/document/**
  - crates/app-core/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
  - docs/ARCHITECTURE.md
---

## 目标

给按 LizzieYzy Next 布局重做的主窗口（P1-20）提供左栏和右栏需要的数据。界面只负责画，不在 TS 里算规则或解析 SGF。

## 要做

- 对局信息：`DocumentView` 增加对局信息，内容如下。根节点缺字段时为空，贴目沿用 P1-03 的宽松规则。
  - 黑白双方名字和段位；
  - 贴目、规则、让子、结果；
  - 日期、赛事名。
- 变化树：
  - 新增整棵棋谱树的视图，供右栏画变化树。每个节点给出：id、父节点、着手（颜色和坐标，pass 和空节点也要能表示）、手数、子节点顺序。第一个子节点是主线。
  - 树结构没变时不要每次都重发整棵树：可以拆成单独的事件，也可以带版本号让界面判断。具体怎么做由实现决定，写进 ARCHITECTURE.md。
- 变化预览：
  - 新命令：给定当前节点和一串着手（候选点的 PV），返回依次落下后的局面，包括提子；同时返回每个预览子的序号，供棋盘按序号显示。
  - 不合法的着手到此为止，返回已经落下的部分。
  - 预览只是看，不改棋谱、不进撤销栈、不触发分析。
- 重新生成 TS 绑定，mock-backend 跟着更新。

## 验收标准

1. 无界面集成测试：
   - 打开语料里带变化和对局信息的棋谱，对局信息和树结构与 SGF 一致；
   - 在变化里落子后，树里多了这个节点，父子关系正确。
2. 变化预览：
   - 有提子的 PV，返回局面里被提的子已经拿掉；
   - 中途不合法的 PV 只返回合法的前半段；
   - 预览前后，文档 revision 和撤销栈都不变。
3. 绑定一致性测试通过，`scripts/verify.sh` 通过。
4. `cargo test -p app-core` 连续跑 10 遍，再在 CPU 满载下连跑 10 遍，全部通过。
