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

- `Point { x: u8, y: u8 }`：左上角原点、零起算；`Color { Black, White }`、`Move { Play(Color, Point), Pass(Color) }`。
- `BoardSize { width: u8, height: u8 }`：2..=25，允许非正方形（KataGo 默认编译只支持到 19，超出由引擎报错）。`BoardSize::new` / `validate` 返回 `BoardSizeError`；局面构造和坐标转换也验证公开字段，不能靠直接构造绕过尺寸限制。
- `Rules { ko: KoRule, allow_multi_stone_suicide: bool, scoring: ScoringRule }`：`KoRule::{Simple, PositionalSuperko, SituationalSuperko}`；`ScoringRule::{Area, Territory}`。这里不计算终局分数。
- `RulePreset::from_name` 接受下表的精确小写名称，未知名称返回 `UnknownRules`；`rules()` 得到上述规则子集，`as_katago_str()` 得到同名 KataGo 规则串。预设身份独立保留，因为日本和韩国规则在本卡建模的字段相同；`Rules::from_name` 只取得规则子集。映射依据 [KataGo rules.cpp](https://github.com/lightvector/KataGo/blob/master/cpp/game/rules.cpp)。

  | 名称 / KataGo 规则串 | 劫 | 允许多子自杀 | 计分方式 |
  |---|---|---|---|
  | chinese | 简单劫 | 否 | 数子 |
  | japanese | 简单劫 | 否 | 数目 |
  | korean | 简单劫 | 否 | 数目 |
  | aga | 情境超级劫 | 否 | 数子 |
  | tromp-taylor | 位置超级劫 | 是 | 数子 |
  | new-zealand | 情境超级劫 | 是 | 数子 |

- `Position::new(size, rules) -> Result<Position, BoardSizeError>` 创建黑先空盘；`with_setup(size, rules, to_play, stones)` 接受 `(Color, Point)` 序列，作为全新的历史起点，不执行提子。越界、重复点、零气块返回 `SetupError`。`stones()` 为左上到右下逐行的只读数组，`stone_at` / `liberties_at` 对空点或越界返回 `None`；后者计算整块的不同气点数。
- `Position::play(mv) -> Result<PlayOutcome, IllegalMove>`：先提对方，再判断己方自杀，最后判断重复。`PlayOutcome.captured` 是对方被提点，`self_captured` 是自杀移除点（含本手新子），均按行排序。被占、自杀、劫、越界返回带原因的 `IllegalMove`，失败不改变棋盘、轮走方、哈希或历史。
- `Position::play_unchecked_ko(mv)` 供 SGF 回放用，只跳过劫检查；仍判越界、被占和自杀，照常提子，并将结果记入历史。两个入口都采用 `Move` 指定的颜色，允许棋谱中连续同色着手，成功后轮走方为该颜色的对方；轮次约束由调用方决定。
- 自杀语义与 KataGo 一致：单子自杀在任何规则下都非法；`allow_multi_stone_suicide` 对应 `multiStoneSuicideLegal`，只控制是否允许移除至少两颗己方棋子（含本手新子），合法多子自杀仍受劫规则限制。六种预设中仅 tromp-taylor 和 new-zealand 开启此项。正常落子和 `play_unchecked_ko` 都执行同样的自杀检查；预设字符串仍原样传递给引擎。
- 简单劫禁止立即回提单子劫；pass 清除此限制。位置超级劫比较盘面，情境超级劫比较盘面加下一轮走方；历史哈希集合包含初始局面和每个成功着手的结果。pass 总是合法，也记入历史；连续 pass 不在本层关闭局面。
- `Position::hash() -> u64`：固定种子生成的 Zobrist（尺寸 + 盘面 + 轮走方），落子和移除棋子以 XOR 增量更新；`recompute_hash()` 从棋盘重算。哈希不含规则和着手历史，不能代替 document 的 `AnalysisKey`。
- 坐标方法：`Point::to_gtp(size)` / `Point::from_gtp(text, size)` 和 `to_sgf` / `from_sgf`；`Move` 提供同名方法，解析时额外传入颜色。19 路 `(15, 3) ⇄ GTP "Q16" ⇄ SGF "pd"`。GTP 跳过 I，按棋盘高度翻转行号，输入不区分大小写，拒绝空白和前导零；SGF 只接受小写坐标。pass 输出为 GTP `pass`、SGF 空值；宽高都 ≤19 时兼容 SGF `tt` 为 pass，更大棋盘按实际点或越界处理。`Point` 不表示 pass，解析 pass 返回 `CoordinateError::PassHasNoPoint`。

### 4.2 sgf

- `parse(bytes: &[u8]) -> Result<Collection, SgfError>`：`Collection { games: Vec<GameTree>, encoding: String, warnings: Vec<Warning> }`。`GameTree { sequence: Vec<Node>, children: Vec<GameTree> }` 保留 SGF 的非空节点序列和有序变化；`Node { properties: Vec<Property> }`、`Property { identifier: String, values: Vec<String> }` 保留属性顺序、重复属性及非空值列表（值可以为空字符串）。
- 编码顺序为 BOM → 合法 UTF-8 → 第一盘根节点的 `CA` → GB18030 兜底。支持 UTF-8 / UTF-16LE / UTF-16BE BOM；有 BOM 的损坏内容直接报错。`CA` 通过 `encoding_rs` 支持 GBK / GB18030、Big5、Shift_JIS、EUC-KR 等明确声明；不进行统计猜测。解码不插入替代字符；声明未知或与实际采用的解码器不一致记 warning。`encoding` 记录所用解码器，带 BOM 时加 `-BOM` 后缀；未声明的 GBK 字节无法与 GB18030 子集区分，统一报告 `GB18030`。整个 collection 使用同一个字节编码。
- 宽松恢复：全小写属性名转大写，FF3 混合大小写长名仅保留大写字母；重复属性保留为独立条目。EOF 缺少树的右括号时补齐。未转义的 `]` 仅在后面不是合法 SGF 边界、且下一个未转义 `]` 位于 `;` / `(` / `)` 之前时作为文本恢复；有歧义时优先采用合法 SGF 结构，不跨节点猜测。每个恢复点记录 `Warning { position, kind }`。
- `SgfError { position, kind }` 是带位置的类型化错误。位置是零起算字节偏移：语法错误和 warning 相对于去 BOM 后的 UTF-8 解码文本，解码错误相对于原始输入。缺失属性值、未闭合值、无节点的树、变化后继续同一序列等不可恢复语法返回错误。变化树最多嵌套 256 层（根为第 1 层），超限返回错误；普通着手序列扁平存储，解析和序列化使用显式栈。
- 属性值仅做 SGF 反转义：反斜杠续行删除，CR / CRLF / LFCR 统一为 LF，其余空白及文本保留；不按 Text / SimpleText 类型裁剪。未知属性（包括 Java 版的 `LZ` 分析属性）同样保留。坐标、贴目、规则的语义解析放在 document 层。
- `serialize(&Collection) -> String`：对合法构造的树输出 UTF-8，正确转义 `]` 和 `\`，换行统一为 LF；不修改传入数据。每盘根节点的第一个 `CA` 原位置写为 `CA[UTF-8]`，去除其余根 `CA`，缺失则追加；非根 `CA` 原样保留。往返树比较仅对根 `CA` 做这一必需的规范化，其余属性、值及变化结构逐项相同；解析后的 `LZ` 文本逐字节一致。

### 4.3 document

- `GameDocument` 是纯同步 arena 树，节点对外只读；`NodeId(u32)` 在单文档生命周期内稳定且永不复用，删除/撤销新建后保留墓碑。通过 `root()`、`current()`、`revision()` 读取状态，`node(id)` 读取父节点、有序子节点、原始 SGF 属性和首个注释。`new()` 创建默认 19 路、中国规则、7.5 目、黑先的空文档。
- `from_sgf(&sgf::GameTree) -> Result<GameDocument, DocError>` 导入一盘；collection 由调用者逐盘选择。序列与变化用显式栈展开，不丢失空节点、属性顺序、重复属性或未知属性。`to_sgf() -> sgf::GameTree` 合并单子节点链为序列，保留节点和变化顺序；UTF-8/CA 规范化继续由 `sgf::serialize` 负责。原始 `KM` 不被归一化值覆写，避免反复导入时二次换算。
- `Command` 包括 `Play(Move)`、`GoTo(NodeId)`、`Step(i32)`、`SetComment(String)`（当前节点）、`DeleteNode(NodeId)`（整棵子树）、`PromoteVariation(NodeId)`（移到同级第一位）。`Play` 先完整检查轮走方及落子规则，有相同着手的直接子节点则前进，否则追加变化；已有 SGF 子节点也不能绕过新下棋的劫检查。`GoTo` 允许访问非法记录以检查原文；`Step` 正数沿第一个子节点、负数沿父节点，到边界即停。不能删除/提升根。删除当前节点的祖先时焦点退到被删子树的父节点，否则不改焦点。注释修改合并当前节点的全部 `C` 为一个，保留其他属性。
- `apply(Command)`、`undo()`、`redo()` 返回 `Result<Revision, DocError>`，采用可逆内容修改增量栈。仅新建节点的 `Play`、`SetComment`、`DeleteNode`、`PromoteVariation` 进入撤销栈，并清空 redo；`GoTo`、`Step` 和前进到已有子节点的 `Play` 只更新 `current`，不入撤销栈，也不清空 redo。每次实际改变内容或焦点，包括撤销/重做，都使 `revision: u64` 加 1；无操作不入栈、不增加版本，失败不改变文档。撤销/重做把焦点恢复到该条内容修改记录的修改前/修改后位置，不受后来导航影响。`DocumentSnapshot` 保存活动节点、根和焦点用于渲染/逻辑比较，不包括递增版本、历史栈和墓碑；全撤销/重做恢复全部内容及对应修改记录的焦点，不回拨版本或 ID 分配器，也不恢复操作期间的导航轨迹。
- `position_at(NodeId) -> Result<GamePosition, DocError>` 按祖先链重算，不维护可失效的局面缓存。`GamePosition` 包含 `board: go_core::Position`（棋盘及轮走方）、完整根起着手 `moves`、累计 `captured_by`（自杀提子归对方）、有效 `rules: RulePreset`、原文 `rules_raw`、`komi`、`handicap`，以及供分析引擎重放的 `initial_stones`、`initial_player`、`analysis_moves`。遇到中途摆子或实际改变轮走方的 `PL` 时，以修改后的棋盘建立新的规则历史和分析起点，清空 `analysis_moves`，完整 `moves` 和提子计数仍保留；`AE` 不算提子。
- SGF 对局信息采用容错读取：根 `SZ` 支持矩形，缺失、不可解析或超出 go-core 尺寸范围时采用 19×19；`HA` 缺失、空或不可解析时采用 0，`HA >= 2` 默认白先。`RU` trim 并转小写后经 go-core 六种预设映射，缺失或不认识则使用 Chinese，原文始终保留。根 `SZ/KM/RU/HA` 和任意节点 `PL` 的重复属性或多值列表，只取第一个属性的第一个值；非根 `SZ/KM/RU/HA` 忽略。`PL` 接受不区分大小写的 B/W，其余值忽略，不阻断回放。任意节点 `AB/AW/AE` 支持单点和矩形压缩范围，以摆子修改而非落子提子处理；摆子和 `PL` 在同节点的 `B/W` 之前应用。着手和摆子的错误仍严格处理：重复/多值着手属性、同节点 `B` 与 `W`、重叠摆子列表、缺气摆子和非法坐标均为节点级错误。
- 贴目先读根 `KM`；缺失、空、解析失败或非有限时尝试根 `KO`（兼容 Cyberoro 等来源，同样只取首值），仍无法使用则采用 7.5。换算后 `abs(komi) >= width * height` 直接采用 7.5。换算保留 Java `SGFParser` 的嵌套条件：根 `AP` 不区分大小写包含 `foxwq` 才认定野狐；野狐 `abs(KM) >= 50` 时先除 100，仅在此分支且原 `RU` 包含 `chinese`、缩放结果在 [-4,4] 时再乘 2。其他棋谱仅 `KM >= 200` 时先除 100，并在此分支内将 [-4,4] 的结果乘 2，随后按 Java 十进制 `.25/.75` 后缀规则乘 2。`KO` 采用同样换算和范围校验。原始贴目属性始终保留，导出不写回回退值。
- `warnings() -> Vec<DocumentWarning>` 返回整棵活动树的文档级诊断，`DocumentWarning { node, property, kind }` 明确指出节点和属性。`WarningKind` 区分重复/多值取首值（`FirstValueUsed`）、解析失败回退（`InvalidValue { fallback }`）、越界回退（`OutOfRange { fallback }`）、未知规则回退（`UnknownRules { fallback }`）、忽略非根信息（`IgnoredNonRoot`）、忽略无效 PL（`IgnoredPlayer`）、PL 大小写规范化（`NormalizedCase`）、使用 KO 贴目（`KomiFromKo`）。普通缺省字段不警告；采用 KO 会警告，非法 KM 回退 KO 时同时记录 KM 原因及 KO 来源。同一属性的多种恢复原因分别记录。诊断按稳定节点 ID 和属性解析顺序返回；读取时重算，不改变 revision、不重复累积，删除子树移除其诊断，撤销恢复诊断；非法着手不妨碍收集其后代或其他分支的元数据诊断。
- 所有 SGF 记录的着手使用 `play_unchecked_ko`，照常提子和检查占用/自杀，允许连续同色。`node_error(id)` 按回放结果标记非法节点及后代，返回带原始出错 `NodeId` 和原因的 `DocError`；`position_at` 对这些节点报错，其他兄弟变化仍可正常回放。导入本身保留非法节点和原属性，不伪造可分析局面。
- `GamePosition::analysis_key()` / `AnalysisKey::for_position` 返回 BLAKE3 32 字节稳定哈希。v1 编码固定为前缀 `lizzieyzy-next2:analysis-key:v1\0`，然后 width/height/初始轮走/当前轮走各 u8；摆子数量 u64 小端，按 (y,x,color) 排序后每子 color/x/y；完整 `moves` 与 `analysis_moves` 各为 u64 小端数量后接着手编码（pass 为 0/color，落子为 1/color/x/y）；规则标签 Chinese/Japanese/Korean/Aga/TrompTaylor/NewZealand 为 0..5；贴目为 IEEE-754 f64 位表示的 u64 小端，负零规范为正零。颜色 Black/White 为 0/1。不包含 NodeId、revision、注释、原文规则拼写和提子统计；固定测试向量锁定编码，不能用盘面 Zobrist 或 `DefaultHasher` 替代。

### 4.4 katago-protocol

- 请求 `AnalysisQuery`：`id`、`moves`、`initialStones`、`initialPlayer`、`rules`、`komi`、`boardXSize`、`boardYSize`、`analyzeTurns`、`maxVisits`、`includeOwnership`、`includePolicy`、`reportDuringSearchEvery`、`firstReportDuringSearchAfter`、`priority`、`priorities`、`overrideSettings`（serde 字段名与 KataGo 一致）。`priorities` 与 `analyzeTurns` 一一对应，录制中按优先级依次返回第 1、2、3、4、0 手。
- `rules` 接受规则名或自定义 JSON 对象；着手为 `(Player, String)`，坐标和合法性留给引擎判断。可选字段用 `Option` 保留是否提供的区别；请求的未知扩展字段保留，以便语义完整地重放录制。
- `Request` 区分 `Analysis` 和 `Action`；动作包含独立的 `id`：`terminate`（另含 `terminateId`）、`terminate_all`、`query_version`。动作确认由 `ActionResponse` 表达；版本回复还含 `version` 和 `git_hash`（下划线拼写）。
- 响应 `AnalysisResponse`：`id`、`turnNumber`、`isDuringSearch`、`moveInfos`、`rootInfo`、可选的 `ownership`、`policy`。候选点与根节点保留录制中的统计字段；所有数值原样读取，黑方视角由引擎配置保证。
- `EngineLine` 分别表示分析结果、`NoResults`、错误、警告、动作确认。错误行允许只有 `error`，`id` 和 `field` 可选；警告同理。排队中被取消的查询以独立的 `NoResultsResponse` 表达：`noResults: true`、`isDuringSearch: false`，没有 `rootInfo` 和 `moveInfos`，不能当成零 visits 的分析结果。
- `parse_line(&str) -> Result<EngineLine, ProtocolError>`：一行一个 JSON；响应中不认识的字段忽略；坏行或缺失必要字段返回类型化错误，不 panic。
- 以 [KataGo 官方协议文档](https://github.com/lightvector/KataGo/blob/master/docs/Analysis_Engine.md) 和 `fixtures/katago/` 的真实录制为准；本文与录制不一致时以录制为准，并更新本文。

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
- `AnalysisRequest` 保留 4.4 中除 `id` 外的全部查询字段；扩展字段不得覆盖协议保留字段（包括 `id` / `action`）。`QuerySubscription::id() -> &str` 是该订阅稳定的取消标识，`recv(&mut self) -> Option<QueryEvent>` 按事件顺序读取，结束后返回 `None`。重发结果中的 wire id 会改变，`terminate` 始终使用订阅的初始 id；对已经结束或未知 id 取消是幂等成功。
- `TransportSpec::LocalProcess { program, args, cwd, env }`。传输层把进程 I/O 变成"发出行 / 收到行 / 退出"三个 channel；以后的 SSH、WebSocket 只需提供同样的三个 channel。
- 本地传输 driver future 由会话 actor 直接轮询，不另起持有进程的任务；参数和环境直接传给子进程，不经过 shell。发出行与收到行各 64 槽，退出为 oneshot；退出报告在 `child.wait()` 和管道排空后产生，包含退出状态及 stderr 尾部。actor 处理退出报告前还会消费已入队的结果，避免遗漏退出前的 Final。关闭先关 stdin，`shutdown_timeout` 后强杀，再等待实际退出；`shutdown()` 成功后状态才为 Stopped。最后一个 handle 被丢弃也会执行清理，status receiver 和查询订阅不会使 actor 永久存活。
- QueryId 格式为 `<会话实例号>-<序号>`，重启后旧 id 永不复用。
- 实例号使用 UUID v4，序号由 actor 持有，动作与查询共用递增序列；重发按原始提交序号排列，保留同优先级请求的输入顺序。
- 就绪：启动后发 `query_version`，收到回复才算 Ready；超时（默认 120 秒，首次 GPU 调优可能很慢）进入重启流程。
- 看门狗：有进行中的请求时，超过 `response_timeout` 没有任何输出，视为挂死并重启。
- 重启：指数退避，有次数上限。明显的配置错误（模型或配置文件不存在、启动即退出）直接 Failed，带 stderr 最后 200 行。重启后把进行中的请求用新 id 重发，订阅者收到一次 `Restarted` 后继续收结果。
- `SessionConfig` 默认：`startup_timeout=120s`、`response_timeout=30s`、`write_timeout=5s`、`shutdown_timeout=2s`、`restart_backoff=250ms`、`max_restart_backoff=10s`、`max_restarts=3`、`max_consecutive_bad_lines=3`。超时均可配置为 `(0, 24h]`；重启上限允许 `0..=32`，是整个会话的总预算，Ready 不重置。退避期间和首次启动期间接受排队提交；尚未发给旧进程的请求不会收到 Restarted。启动失败、重启耗尽时保留 Failed 状态供读取，后续提交返回错误，直到 shutdown 或所有 handle 丢弃。
- 背压：订阅者跟不上时只保留最新一个 Partial；Final / Failed 永不丢。
- actor 命令 channel 固定 64 槽、待发队列最多 256 行、同时进行的查询最多 128 个，满载提交返回 `SessionError::Capacity`。每个订阅是一个 watch 槽：一个最新 Partial 加有界可靠事件历史，慢消费者不阻塞 actor；终态在 actor 退出后仍可读。单请求最多 256 个 `analyzeTurns` 项，每个去重手数都保留 Final，仅重发尚未完成的手数并保留对应优先级；可靠历史至多 256 个 Final、32 个 Restarted 和一个终态。取消、重启或失败清除旧 Partial。查询错误只结束对应订阅；未取消查询意外收到 noResults 时报告类型化失败，绝不制造分析值。
- 坏行：记录并计数，不崩溃；连续坏行超过阈值视为协议故障，触发重启。
- 有效协议行重置连续坏行计数；累计坏行数另外用于 tracing 日志。stdout 或 stderr 的任何输出均刷新进行中请求的看门狗；空闲时不运行响应看门狗。tracing 记录请求标识、重发标识、完成／取消耗时和故障原因。
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

### fake-katago 测试进程约定

- 启动形状为 `fake-katago analysis -config FILE -model FILE`；只接受路径参数，不读取模型或配置，也不调用真实 KataGo。输出明确标识版本为 `1.16.5-fake`。
- 单个调度器管理排队与运行中的搜索；`--threads` 指定并发位置数（默认 1）。优先级高的先开始，相同优先级按输入顺序；已开始的搜索不会被抢占。stdin 关闭后排空正常搜索再退出。
- 同一请求的 `analyzeTurns` 按手数去重、递增入队，重复手数采用最后一个对应优先级；`priority` 与 `priorities` 不得同时提供。只有 `firstReportDuringSearchAfter` 时仍输出一次首次中间报告。未指定初始玩家且无历史时，超过一颗初始黑子、没有白子则默认轮白，其余默认轮黑。
- 候选点及评估由棋盘尺寸、初始摆子、着手序列和手数的固定哈希生成，与请求 id 无关。默认 `maxVisits=100`、`--visits-per-sec=1000`；visits 随模拟时间增长，最终结果达到上限，取消则保留已有 visits。评估和 ownership 均固定黑方视角。
- `--time-scale` 表示每个真实秒经过多少模拟秒（默认 1，2 表示两倍速）；搜索、报告间隔、`--startup-delay-ms` 和 `--crash-after-ms` 都使用模拟时间。启动与定时崩溃从进程调度器初始化开始计时。
- `--crash-after-queries N` / `--hang-after-queries N` 在接受第 N 个有效分析请求时触发，动作不计数，0 表示启动即触发。崩溃退出码为 1；挂起后连版本回复、取消确认也不再输出，直到进程被终止或定时崩溃触发。
- `--fail-startup` 立即向 stderr 写入“模型找不到”并以退出码 1 退出；`--garbage-every N` 在每 N 条正常 stdout 协议行后额外插入一行坏数据，故意用于测试会话的坏行处理。
- 取消顺序遵循录制：排队结果先 `noResults`，然后动作确认，最后是运行中查询的最终结果；`terminate_all` 同时取消所有排队与运行中的位置。
- 假引擎检查坐标、已有棋子、提子、自杀和简单劫；不作为完整规则裁判，不模拟超级劫、真实搜索、模型设置或对称剪枝。录制对照比较公共字段与状态顺序；重复中间采样数量和条件性 `isSymmetryOf` 不要求与 GPU 搜索逐项相同。
- 测试库函数 `fake_katago::binary_path() -> Result<PathBuf, BinaryPathError>` 首次调用执行 `cargo build -p fake-katago --bin fake-katago --locked`，从 Cargo JSON 消息取得可执行文件路径，尊重目标目录及平台后缀。同一进程用互斥锁串行初始化，其他调用者等待后直接复用成功缓存的路径，避免并发构建删除并重新链接可执行文件的竞争；构建失败不缓存，后续调用可重试。仅供运行时测试调用，不用于构建脚本或产品代码。

## 8. 与 Java 版的兼容

- 读写 SGF 的 `LZ` 分析属性（格式以 Java 版 `SGFParser`、`SgfAnalysisPayloadWriter` 为准，P2）。
- 编码：BOM、`CA`、UTF-8、GB18030 / GBK（参考 Java 版 `EncodingDetector`）。
- GIB（弈城）导入（P2）。
- 首次启动导入 Java 版的配置和引擎设置（P2）。
- 应用标识 `io.github.wimi321.lizzieyzy.next2`，配置目录独立，可与 Java 版同时安装。

## 9. 暂不做

GTP 后端、人机对局、陪练、readboard、远程算力、弈客、棋力评估、AI 解说、死活题、网页棋盘，见 ROADMAP 的 P3。
