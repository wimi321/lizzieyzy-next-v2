---
id: P1-19
title: 野狐昵称抓谱：后端
depends_on: [P1-08]
paths:
  - crates/fox-kifu/**
  - crates/app-core/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
  - fixtures/fox/**
  - fixtures/sgf/**
  - Cargo.toml
  - scripts/architecture.json
  - docs/ARCHITECTURE.md
---

## 目标

和 Java 版一样：输入野狐昵称或 UID，拿到这个账号最近的公开棋谱，选一盘直接打开分析。

维护者试用 P1 后要求提前做这个功能。野狐昵称抓谱原本排在 P2，但维护者日常复盘就是从野狐抓谱开始的。这张卡只做后端和命令，界面是 P1-22，由 Claude 做。

## 要做

- 行为参照 Java 版：
  - `src/main/java/featurecat/lizzie/analysis/GetFoxRequest.java`；
  - `src/main/java/featurecat/lizzie/gui/FoxKifuDownload.java`；
  - 两者的测试：分页游标、段位、响应解析、找不到用户。

  只参照行为和接口知识，不照搬结构。Java 版走的是野狐公开的 HTTP 接口，不需要 `FoxRequest.jar`。
- 新建 crate `crates/fox-kifu`，负责网络请求和解析，不依赖其他工作区 crate：
  - 昵称 → 账号。昵称不存在时返回明确的"没有这个账号"，和网络错误区分开。
  - 也支持直接输入 UID，只允许数字。
  - 账号 → 最近公开棋谱列表，支持翻页，没有更多时明确告知。每项包含：对局 id、黑白双方昵称和段位、结果（含中盘胜、超时胜等胜负方式）、手数、对局时间。Java 版列表里有的信息都要有。
  - 段位按 Java 版规则格式化（段、级、职业 P）。
  - 对局 id → SGF 文本。主接口失败时，按 Java 版的顺序回退到备用接口。
  - HTTP 用 rustls，请求有超时。所有错误都是类型化的。
- 在 `scripts/architecture.json` 里登记新 crate。这张卡只允许改两处：
  - `workspace_crates` 加 `"fox-kifu": []`；
  - `app-core` 的依赖列表加 `"fox-kifu"`。
- app-core 增加以下命令：
  - 按昵称或 UID 搜索，返回账号和第一页棋谱；
  - 翻下一页；
  - 按对局 id 下载并打开，走现有的打开流程，打开后的文档和打开本地 SGF 一样。
- 近期搜索记录存进设置，最多 10 条，最新的在前，可以清空。
- 提供对应的 Tauri 命令和 ts-rs 绑定，mock-backend 跟着更新。

## 验收标准

1. 单元测试：
   - 账号查询：找到、找不到、UID 格式不对、接口报错；
   - 列表解析：含分页游标、段位、各种结果；
   - SGF 下载与回退。
2. 测试不连真服务器。用 `fixtures/fox/` 里的录制响应，用本地假 HTTP 服务回放。
3. app-core 集成测试：从昵称一路到打开棋谱，`DocumentChanged` 里是这盘棋。
4. 至少一份真实的野狐 SGF 样本进 `fixtures/sgf/`（这是 P0-03 遗留的样本），测试能正确打开，双方、结果、手数都对。
5. 联网实测一次：用 Java 测试或文档里出现过的真实昵称，列出最近棋谱并下载一盘。这个测试标成 `#[ignore]`，不进 CI。结果贴进汇报。
6. `scripts/verify.sh` 通过，绑定一致性测试通过。
