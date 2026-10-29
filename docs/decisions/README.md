# 决策记录

每条写清决定、原因、代价。新决定追加在末尾，编号不复用；推翻旧决定时新增一条并注明。

## D-001 技术栈：Tauri 2 + Rust 内核 + React/TypeScript 界面

- 原因：安装壳小（旧 Tauri 版安装包 4–7MB，Java 版不带引擎 316MB）；Rust 编译器强制单一所有者；网页界面可复用到网页棋盘和浏览器版；系统 WebView 取代捆绑的 Chromium（JCEF）。
- 代价：维护者要熟悉 Rust；macOS 没有官方 WebDriver，全栈 E2E 只在 Linux / Windows 跑。

## D-002 KataGo 只走分析引擎 JSON 协议

- 原因：每个请求自带完整局面，引擎里没有需要同步的状态。Java 版 #429、#523、#560 这类问题来自 GTP 状态同步和前后台仲裁。实时分析用 `reportDuringSearchEvery` + `terminate`，整盘用 `analyzeTurns` + `priority`，HumanSL 用 `overrideSettings`。
- 代价：第三方 GTP 引擎和 `genmove` 对局要另做 GTP 后端（P3）。

## D-003 每个引擎一个 actor，引擎相关 crate 禁止锁

- 原因：Java 版 Leelaz 约有 446 处 synchronized、27 个租约 / 许可类，每修一个竞态就多一个对象。
- 代价：所有交互都要设计成消息；调试依赖日志和 tracing。

## D-004 Rust 内核是唯一事实来源，前端只是视图，TS 类型由 Rust 生成

- 原因：旧 Tauri 版在 TS 里重复实现了 SGF 解析等逻辑，两边的 DTO 会漂移。
- 代价：前端的每个新需求都要先有后端接口。

## D-005 分析数值一律黑方视角

- 原因：避免每个界面组件各自翻转视角出错。应用自带的 analysis.cfg 显式设置 `reportAnalysisWinratesAs = BLACK`。

## D-006 验收以可执行的行为测试为准

- 原因：旧 Tauri 版有 4.2 万行 Python 门禁，其中大量是对源码做正则匹配，证明不了功能可用。
- 代价：要先投入做 fake-katago 和 E2E。

## D-007 许可证 GPL-3.0

- 原因：新版沿用 LizzieYzy 的行为、翻译和协议知识，属于衍生作品；与 Java 版一致。

## D-008 新仓库，不在 lizzieyzy-next-tauri 上继续

- 原因：旧仓库的 `lib.rs` 单文件 1.1 万行，每次分析都新起一个 KataGo 进程，还有大量证据型代码。只把它的 golden SGF 和发布 workflow 当参考。
