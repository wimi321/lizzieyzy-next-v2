# 架构

新版的目标架构和接口约定。任务卡引用这里的章节；改动这里描述的接口时，必须同步更新本文。

## 1. 目标

- 以复盘为核心的 KataGo 围棋分析工具，桌面端 Windows / macOS / Linux。
- 安装壳小（目标 < 15MB，不含引擎和模型）。
- 引擎层没有共享锁：每个引擎一个 actor，前后台分析不再靠"租约 / 许可"类对象仲裁。
- 与 Java 版并存：棋谱、分析数据、配置可以互通（第 8 节）。

## 2. 仓库结构

```
Cargo.toml                 Rust workspace
crates/
  go-core/                 棋盘、规则、坐标、Zobrist 哈希（纯同步）
  sgf/                     SGF 解析 / 序列化、编码识别（纯同步）
  document/                棋谱树、编辑命令、撤销、局面计算（纯同步）
  katago-protocol/         分析引擎 JSON 协议类型与解析（纯同步）
  engine-session/          每个引擎一个 actor：进程、请求表、重启（tokio）
  analysis/                分析协调：焦点变化、过期丢弃、缓存、节流（tokio）
  app-core/                应用内核 actor：持有文档、协调器、设置；对外唯一入口（tokio，不依赖 tauri）
apps/desktop/
  src-tauri/               Tauri 壳（包名 lizzieyzy-desktop）；commands/<领域>.rs 只做转发
  src/                     React + TS 界面
    api/                   backend.ts（对生成绑定的薄封装）、generated/（由 Rust 生成，勿手改）
    board/ graph/ toolbar/ settings/ state/ styles/ locales/
  e2e/                     Playwright 用例与 mock 后端（不进生产包）
tools/fake-katago/         测试用假 KataGo（Rust 二进制 + binary_path() 库函数）
fixtures/                  SGF 语料、KataGo 真实录制、期望结果
resources/katago/          应用自带的 analysis.cfg 模板
scripts/                   验收门禁（维护者所有）
docs/                      ARCHITECTURE / ROADMAP / ACCEPTANCE / decisions / tasks / inventory
```

## 3. 依赖方向

```
go-core ← sgf ← document ←──────────── analysis ← app-core ← lizzieyzy-desktop
katago-protocol ← engine-session ←───┘
katago-protocol ← fake-katago（engine-session / analysis / app-core / desktop 仅在测试里依赖它）
```

准确的允许列表在 `scripts/architecture.json`，由 CI 检查。纯同步 crate 不依赖 tokio、tauri、网络、数据库，以后可以编译到 WASM。

## 4. 关键接口（草案：实现时可以细化，语义不能变）

### 4.1 go-core

- `Point { x: u8, y: u8 }`、`Color { Black, White }`、`Move { Play(Color, Point), Pass(Color) }`。
- `BoardSize { width: u8, height: u8 }`：2..=25，允许非正方形（KataGo 默认编译只支持到 19，超出由引擎报错）。
- `Rules`：劫（简单劫 / 位置超级劫 / 情境超级劫）、是否允许自杀、数子或数目。规则名（chinese、japanese 等）与 KataGo 规则串的映射放这里。
- `Position::play(mv) -> Result<PlayOutcome, IllegalMove>`，`PlayOutcome` 含提子列表；`Position::play_unchecked_ko(mv)` 供 SGF 回放用（照常提子，不判劫）。
- `Position::hash() -> u64`：Zobrist（盘面 + 轮走方），超级劫用历史哈希集合判断。
- 坐标转换：`Point ⇄ GTP（"Q16"，跳过 I）⇄ SGF（"pd"）`。

### 4.2 sgf

- `parse(bytes: &[u8]) -> Result<Collection, SgfError>`：输入是字节，内部识别编码（BOM → 合法 UTF-8 → `CA` → GB18030 兜底）；宽松模式能恢复的错误记入 `warnings`。
- 未知属性原样保留（包括 Java 版的 `LZ` 分析属性），序列化时写回。
- `serialize(&Collection) -> String`：输出 UTF-8，根节点写 `CA[UTF-8]`。
- 只保存属性的原始文本；坐标、贴目、规则的语义解析放在 document 层。

### 4.3 document

- `GameDocument`：arena 存储的树，`NodeId(u32)` 稳定且不复用；`root`、`current`、`revision: u64`（每次修改加 1）。
- 命令：`Play(Move)`、`GoTo(NodeId)`、`Step(i32)`、`SetComment`、`DeleteNode`、`PromoteVariation`……；`apply(cmd) -> Result<Revision, DocError>`；撤销 / 重做基于命令栈。
- `position_at(NodeId) -> Result<GamePosition, DocError>`：棋盘、轮走方、从根开始的着手、初始摆子、规则、贴目。
- `AnalysisKey`：对 (尺寸、初始摆子、着手序列、规则、贴目) 做**跨版本稳定**的哈希。不能只按盘面，因为 KataGo 的网络输入包含最近几手。
- SGF 回放时，棋谱里记录的着手不判劫；下在已有子处的节点标记为非法。用户新下的棋按当前规则完整判断。

### 4.4 katago-protocol

- 请求 `AnalysisQuery`：`id`、`moves`、`initialStones`、`initialPlayer`、`rules`、`komi`、`boardXSize`、`boardYSize`、`analyzeTurns`、`maxVisits`、`includeOwnership`、`includePolicy`、`reportDuringSearchEvery`、`priority`、`overrideSettings`（serde 字段名与 KataGo 一致）。
- 动作：`terminate`（`terminateId`）、`terminate_all`、`query_version`。
- 响应 `AnalysisResponse`：`id`、`turnNumber`、`isDuringSearch`、`moveInfos`、`rootInfo`、`ownership`、`policy`；错误行（`error`、`id`、`field`）和警告行解析成各自的类型。排队中被取消的查询会返回 `noResults: true`，没有 `rootInfo` 和 `moveInfos`。
- `parse_line(&str) -> Result<EngineLine, ProtocolError>`：一行一个 JSON；不认识的字段忽略；坏行返回错误，不 panic。
- 以 KataGo 官方文档（KataGo 仓库 `docs/Analysis_Engine.md`）和 `fixtures/katago/` 的真实录制为准；本文与录制不一致时以录制为准，并更新本文。

### 4.5 engine-session

```rust
pub fn spawn(transport: TransportSpec, cfg: SessionConfig) -> EngineHandle;

impl EngineHandle { // 可廉价 clone，只持有 channel
    pub async fn submit(&self, req: AnalysisRequest) -> Result<QuerySubscription, SessionError>;
    pub async fn terminate(&self, id: QueryId) -> Result<(), SessionError>;
    pub fn status(&self) -> watch::Receiver<EngineStatus>;
    pub async fn shutdown(self) -> Result<(), SessionError>;
}
pub enum QueryEvent { Partial(AnalysisResponse), Final(AnalysisResponse), Restarted, Failed(QueryError), Terminated }
pub enum EngineStatus {
    Starting, Ready { version: String }, Restarting { attempt: u32, reason: String },
    Failed { reason: String, stderr_tail: Vec<String> }, Stopped,
}
```

- 一个 tokio 任务独占进程、待完成请求表和状态，外部只能发消息。`AnalysisRequest` 是不含 id 的查询，id 由会话分配。
- `TransportSpec::LocalProcess { program, args, cwd, env }`。传输层把进程 I/O 变成"发出行 / 收到行 / 退出"三个 channel；以后的 SSH、WebSocket 只需提供同样的三个 channel。
- QueryId 格式为 `<会话实例号>-<序号>`，重启后旧 id 永不复用。
- 就绪：启动后发 `query_version`，收到回复才算 Ready；超时（默认 120 秒，首次 GPU 调优可能很慢）进入重启流程。
- 看门狗：有进行中的请求时，超过 `response_timeout` 没有任何输出，视为挂死并重启。
- 重启：指数退避，有次数上限。明显的配置错误（模型或配置文件不存在、启动即退出）直接 Failed，带 stderr 最后 200 行。重启后把进行中的请求用新 id 重发，订阅者收到一次 `Restarted` 后继续收结果。
- 背压：订阅者跟不上时只保留最新一个 Partial；Final / Failed 永不丢。
- 坏行：记录并计数，不崩溃；连续坏行超过阈值视为协议故障，触发重启。
- terminate 的真实行为见 `fixtures/katago/terminate-*.jsonl`：
  - 搜索中取消：先收到确认，再收到该查询的最终结果（`isDuringSearch:false`，带已有的 visits）；
  - 排队中取消：先收到 `{id, turnNumber, isDuringSearch:false, noResults:true}`，之后才收到确认。

  所以不能假设确认先到，也不能假设一定有 `moveInfos`。会话吞掉被取消查询的所有后续输出，订阅者只收到一次 `Terminated`。

### 4.6 analysis（协调器）

- 输入：`Focus { revision, node_id, position, key }`、`SetPondering(bool)`、`SetLiveSettings { … }`；输出 `AnalysisSnapshot`。
- 焦点变化：terminate 旧的实时请求 → 先推缓存里该 key 的最好结果 → 发新的实时请求（`reportDuringSearchEvery`，默认 0.15 秒，可配置）。
- 只转发 (revision, node_id) 等于当前焦点的结果，其余丢弃。这是"旧局面分析串到新局面"的根治手段。
- 节流：每个焦点最多 20 次/秒。
- 缓存：P1 用内存 LRU（AnalysisKey + 引擎指纹，保留 visits 最多的结果），P2 换 SQLite。
- 整盘分析（P2）：`analyzeTurns` 批量提交，`priority` 低于实时请求。

### 4.7 app-core 与 Tauri 层

- `AppCore` actor 持有 `GameDocument`、协调器句柄和设置，所有命令串行处理，保证"文档修改 → 焦点更新 → 分析"的顺序。
- 对外事件：`DocumentChanged { revision, view }`、`AnalysisUpdated(AnalysisView)`、`EngineStatusChanged(EngineStatus)`。
- 视图 DTO（前端只认这些）：
  - `BoardView`：尺寸、每点棋子、最后一手、轮走方、手数、提子数。
  - `TreeView`：P1 只需主线和当前节点的变化数。
  - `AnalysisView`：node_id、revision、根节点胜率 / 目差 / visits、候选点（坐标、visits、胜率、目差、先验、顺序、PV）、可选的所有权数组。
- **数值一律黑方视角**：应用自带的 `resources/katago/analysis.cfg` 显式设置 `reportAnalysisWinratesAs = BLACK`。
- Tauri 层：`commands/<领域>.rs` 每个命令只做参数转换和调用 `AppCore`；事件流用 `tauri::ipc::Channel` 推送；类型用 specta 或 ts-rs 生成到 `src/api/generated/`。

## 5. 主流程时序（导航到下一手）

```
UI 按 →  ──命令──▶  AppCore：document.apply(Step(+1)) → revision 42
                    AppCore ──Focus{42, node}──▶ 协调器
                    协调器：terminate(旧 id) → 推缓存结果 → submit(新查询)
                    engine-session ⇄ KataGo：每 0.15 秒一个 Partial
                    协调器：(42, node) 等于当前焦点？是 → 节流 → AnalysisUpdated
UI  ◀──Channel──    重绘候选点
```

## 6. 前端

- 状态：Zustand store，只由后端事件更新；组件不直接改业务状态。
- 棋盘：分层 Canvas。底层（棋盘、网格、坐标）只在尺寸变化时重画；棋子层在局面变化时重画；分析层（候选点、所有权、PV 预览）随分析流重画。
- 胜率图：Canvas，黑方胜率曲线加目差，点击 / 拖动跳转。
- mock 后端：`apps/desktop/e2e/mock-backend.ts` 实现与 `src/api/backend.ts` 相同的接口；Playwright 模式下由 Vite alias 替换，生产代码不得 import。

## 7. 测试策略

| 层 | 工具 | 测什么 |
|---|---|---|
| 纯 crate | cargo test + proptest | 规则不变量、SGF 往返、golden 语料 |
| 协议 | cargo test | 用 `fixtures/katago/` 真实录制做 golden |
| 会话 / 协调 | cargo test + fake-katago | 取消、重启、乱序、背压、过期丢弃 |
| app-core | cargo test（无界面） | 打开棋谱 → 导航 → 收到正确的分析事件 |
| 前端逻辑 | Vitest | 坐标换算、颜色映射、store |
| 界面 | Playwright + mock 后端 | 交互、截图基线、主线程无长任务 |
| 全栈 | tauri-driver（Linux / Windows CI）+ fake-katago | 主流程 |

## 8. 与 Java 版的兼容

- 读写 SGF 的 `LZ` 分析属性（格式以 Java 版 `SGFParser`、`SgfAnalysisPayloadWriter` 为准，P2）。
- 编码：BOM、`CA`、UTF-8、GB18030 / GBK（参考 Java 版 `EncodingDetector`）。
- GIB（弈城）导入（P2）。
- 首次启动导入 Java 版的配置和引擎设置（P2）。
- 应用标识 `io.github.wimi321.lizzieyzy.next2`，配置目录独立，可与 Java 版同时安装。

## 9. 暂不做

GTP 后端、人机对局、陪练、readboard、远程算力、弈客、棋力评估、AI 解说、死活题、网页棋盘，见 ROADMAP 的 P3。
