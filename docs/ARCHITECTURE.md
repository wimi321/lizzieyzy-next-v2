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
  strength/                棋力特征、XGBoost 推理、残差校准与统计（纯同步）
  fox-kifu/                野狐公开账号、分页棋谱及 SGF 的 HTTP 客户端（不依赖工作区 crate）
  readboard/               棋盘同步的纯同步行协议解码和命令编码
  app-core/                应用内核 actor：持有文档、协调器、设置；对外唯一入口（tokio，不依赖 tauri）
apps/desktop/
  src-tauri/               Tauri 壳（包名 lizzieyzy-desktop）；commands/<领域>.rs 只做转发
  src/                     React + TS 界面
    api/                   backend.ts（对生成绑定的薄封装）、generated/（由 Rust 生成，勿手改）
    board/ graph/ toolbar/ settings/ state/ styles/ locales/
  e2e/                     Playwright 用例与 mock 后端（不进生产包）
tools/fake-katago/         测试用假 KataGo（Rust 二进制 + binary_path() 库函数）
tools/fake-readboard/      独立测试二进制，按文本回放同一 stdio 协议
fixtures/                  SGF 语料、KataGo 真实录制、期望结果
resources/katago/          应用自带的 analysis.cfg 模板与模型身份目录
scripts/                   验收门禁（维护者所有）
docs/                      ARCHITECTURE / ROADMAP / ACCEPTANCE / decisions / tasks / inventory
```

## 3. 依赖方向

```
go-core ← sgf ← document ←──────────── analysis ← app-core ← lizzieyzy-desktop
katago-protocol ← engine-session ←───┘
fox-kifu ← app-core
go-core ← readboard ← app-core
katago-protocol ← fake-katago（engine-session / analysis / app-core / desktop 仅在测试里依赖它）
strength ← app-core（strength 无 workspace 内部依赖）
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
- 只读展示接口：`game_info()` 读取根节点首个属性的首值，提供 PB/BR/PW/WR、RU、HA、RE、DT、EV 和与回放一致的有效 komi。文本缺失为 None、显式空值保留空串，规则保留原文；HA 缺失为 None，存在但无效时沿用 0 的回退。`variation_tree()` 以显式栈按有序深度优先遍历全部活动节点，返回 id、parent、children、move_number、mv、move_error；不含墓碑。手数规则与 GraphView 一致，pass 显式表示、空节点 mv 为 None。错误着手仍保留节点，move_error 只说明本节点的着手语法/坐标解码失败，不表示整条路径已通过规则验证；位置合法性仍由 position_at 判断。
- `preview_variation(node, moves)` 在独立 GamePosition 上按完整规则和轮走方逐手执行，首个非法着手立即停止，返回 position、applied_moves 合法前缀及 stop_reason；无效或非法的基准节点直接返回 DocError。`stones` 只包含最终仍在盘上的预览子及其从 1 起算的 PV 序号，pass 消耗序号但没有标记，被提/自杀移除的子不保留标记，提子后重下同点采用新序号。局面同时更新提子数、轮走方、完整着手及分析重放序列。接口仅借用文档，不改焦点、revision、SGF 或撤销/重做栈。
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

- `analysis::spawn(engine: EngineHandle, config: AnalysisConfig) -> Result<(AnalysisHandle, AnalysisSubscription), AnalysisError>` 在 Tokio runtime 内启动一个协调 actor；直接消费 engine-session 的 `QuerySubscription` / `QueryEvent`，不另起进程管理或结果转发任务。
- `AnalysisHandle` 可 clone，提供异步 `focus(Focus)`、`focus_with_cached(Focus, Option<AnalysisResponse>)`、`set_pondering(bool)`、`set_live_settings(LiveSettings)` 和 `shutdown(self)`，均返回 `Result<(), AnalysisError>`；成功返回表示命令已经应用。`Focus { revision, node_id, position: GamePosition, key: AnalysisKey }` 的 key 必须匹配 position；相同 revision/node/key 的重复焦点命令幂等；当前请求失败时可重复该命令重试。无效输入不改变已有焦点、查询或状态。`focus_with_cached` 接受调用方按局面 key 和引擎指纹核对的文档保留结果，仅在 node/key 改变或尚无已发布快照时注入缓存；不覆盖更深的 LRU 条目，也不替换同焦点已发布快照。LRU 淘汰后重访仍先发布这份结果并建立显示 visits 下限，再接受追平的实时输出；暂停实时搜索时也可以发布 Cache。
- `AnalysisConfig` 包含 `engine_fingerprint: String`、`cache_capacity: usize`（默认 128，必须大于 0）、`live: LiveSettings` 和 `pondering: bool`（默认开启）。指纹由调用方标识引擎版本、模型和影响评估的配置，在协调器生命周期内固定；更换这些内容必须新建协调器。默认空指纹仅适合单个引擎会话，不表示跨引擎结果可复用。
- `LiveSettings { max_visits: u64, report_interval: Duration, include_ownership: bool, include_policy: bool }` 默认分别为 `u32::MAX`、150ms、true、false；visits 必须大于 0，报告间隔必须在 `(0, 24h]`。达到访问上限后结束该请求，不自动循环提交。实际修改设置清除当前焦点的已发布快照记录和 visits 下限，取消旧请求，再用新设置提交；相同设置是无操作，无效设置不改变已有状态。暂停取消当前请求并清掉未读输出，恢复为同一焦点分配新请求；暂停/恢复保留已发布快照记录及 visits 下限。
- 请求使用 `GamePosition.analysis_moves` 配合 `initial_stones`、`initial_player` 重放到当前局面；完整 `moves` 不能用于中途摆子或 PL 重置后的请求。规则使用有效预设字符串，贴目使用文档已规范化值，尺寸来自 board；GTP 坐标由 go-core 转换。只分析最后一手，priority 为 0。文档 warning 仍由 `GameDocument::warnings()` 提供，协调器不重复解析元数据或生成 warning。数值依赖 4.7 的引擎 BLACK 配置，响应原样保留，不按轮走方二次转换。
- node_id 或 key 变化：清除未读的旧焦点输出、待节流结果和已发布快照记录 → 用订阅的稳定 id terminate 旧实时请求 → 先推缓存里该 key 的最好结果 → 发新实时请求（`reportDuringSearchEvery` 和 `firstReportDuringSearchAfter` 均使用 report_interval）。关闭或丢弃最后一个 handle 也取消其当前请求，但不关闭共享的 engine-session。
- 仅 revision 变化、node_id/key 不变：不 terminate、不 submit，保留原订阅并更新查询身份；清掉未读的旧 revision 输出，立即用新 revision 重发最近一次已发布快照（如果有，response 和 source 不变）。待节流结果也更新 revision，包括查询已经完成但尚未发布的 Final；此后所有输出均使用新 revision。已发布快照的 visits 下限不变，查询已完成时也不因 revision 变化重新搜索。
- `AnalysisSubscription::recv(&mut self) -> Option<AnalysisSnapshot>` 是取消安全的输出入口。`AnalysisSnapshot { revision, node_id, key, response: AnalysisResponse, source: SnapshotSource, visits_per_second: Option<f64> }` 的 source 区分 Cache / Live；仅发布 revision/node/key 都匹配当前焦点的数据。输出使用有界 watch 邮箱，保留一个未读缓存结果及一个最新实时结果；慢消费者仍先收到缓存，实时中间结果合并，最终结果保留到新命令使其失效。焦点命令成功返回后，旧焦点尚未消费的数据也不可再读到。
- 显示深度：记录当前焦点实际已发布的最新快照及其 visits 下限，缓存快照也计入。实时结果 root visits 小于下限时直接抑制，不进入待节流槽；相等或更大时正常发布。仅收到但尚未发布的结果不抬高该下限。当前查询收到引擎重启事件时清除下限，保留已显示快照直到新结果到达；重启间隙仅 revision 变化仍重发旧快照，但不恢复旧下限。新引擎首份结果即使更浅也正常发布，并重新建立下限，此后照常不倒退。暂停/恢复保留下限，查询追平后才能更新显示；更浅的 Final 也不发布，但请求正常结束。实际改设置或 node_id/key 变化清除快照记录及下限，返回局面后新发布的缓存重新建立下限；缓存始终保留 visits 更多的结果。
- 节流：同一 revision 的正常分析输出至少间隔 50ms，最多 20 次/秒，缓存也占一次；首条立即发布，中间结果合并到最新，符合显示深度条件的最终结果延后到下个允许时刻发布。忙时不补发错过的时钟。仅 revision 变化时的快照重发是新 revision 的首条，立即发送；保留待发结果并把后续正常输出推迟至少 50ms，没有历史快照可重发时则保留原期限。暂停/恢复及改设置不重置节流时钟；node_id/key 变化清掉旧时钟与待发数据。
- 实时搜索速度由当前查询独占的采样器计算：每份通过焦点身份检查的 Partial 在显示深度过滤和节流前，用 `tokio::time::Instant` 记录到达时间与 root visits；仅保留距最新采样不超过 3 秒的数据，按首末 visits 差除以时间差，跨度不足 500ms 为 None。Final 不加入采样，保留最后一次速度；暂停保留已发布快照中的速度。换 node/key、新查询提交（含恢复和修改设置）、Restarted 均从空采样重算；仅 revision 变化和幂等命令保留。visits 回退时保守清空采样，避免负速或下溢。速度不写入 LRU 或文档保留结果，Cache 和整盘结果均为 None。
- 缓存：P1 用有界内存 LRU（AnalysisKey + 引擎指纹，保留 root visits 最多的结果，相同 visits 更新为后来结果）；读取或刷新提升最近使用顺序，浅结果不会覆盖深结果。缓存使用所有收到的当前查询结果，独立于输出节流。P2 换 SQLite。
- `status()` 返回 `watch::Receiver<AnalysisStatus>`，状态为 Idle / Analyzing / Restarting / Failed(AnalysisError) / Stopped。查询故障以类型化错误报告，不伪造分析值；收到 Restarted 清掉待节流的旧 Partial 和 visits 下限，保留已发布快照，继续读取原订阅，由 engine-session 自动重发，协调器不重复提交。
- `analysis::review::ReviewPlan::new(Vec<(NodeId, GamePosition)>, target_visits)` 为调用方提供的待分析节点建立批量计划。每个查询使用 `analyzeTurns`，最多 256 个不同手数，priority 为 -100（实时为 0），包含 ownership；保留搜索中报告以维持会话活性，但仅达到目标 visits 的 Final 计入完成。沿兼容的重放前缀合批，遇到摆子、PL、规则、尺寸或贴目等重放上下文变化时另起查询；空节点可以共享同一手数，一个 Final 映射回多个各自带 AnalysisKey 的节点。超过 256 手分批，过滤完成节点后仍保留原重放手数，不把节点 ID 当手数。
- `ReviewPlan::into_batches()` 返回 `ReviewBatch`，`submit(&EngineHandle)` 得到由 app actor 持有的 `ReviewRun`。`recv()` 取消安全，返回包含 node/key 和完整 `AnalysisResponse` 的结果；`cancel()` 按稳定订阅 ID terminate 并丢弃未读结果。批次顺序提交，重启交由 engine-session 只重发未完成手数，review 层不另行重发；不足目标的 Final、查询失败或未完成就结束均返回明确错误，不伪造完成。这里不保存文档结果、不修改文档，也不另建引擎任务。

### 4.7 app-core 与 Tauri 层

- `app_core::spawn(settings_directory)` / `spawn_with_config(settings_directory, AppConfig)` 异步加载设置，返回可 clone 的 `AppCore` 命令句柄和单消费者 `AppSubscription`。actor 独占 `GameDocument`、engine-session、协调器和设置，64 槽命令通道串行处理，保证"文档修改 → 焦点更新 → 分析"的顺序。`AppConfig` 提供会话超时和协调器参数；引擎指纹及 pondering 由档案/用户设置决定。
- 命令包括 `open_bytes(Vec<u8>)`、`open_path(PathBuf)`、`save(PathBuf)`、`play(Move)`、`step(i32)`、`go_to(u32)`、`apply(document::Command)`、`undo()`、`redo()`、`set_pondering(bool)`、`configure_engine(Option<EngineProfile>)`、`settings()`、`shutdown(self)`。成功回复表示命令已应用，不等待分析完成；引擎失败通过事件报告，不撤销已成功的文档操作。所有文档实际变化均向协调器发送 `Focus`，仅 revision 变化也照常发送，由协调器保持查询。
- 对外 revision 是应用生命周期内单调增加的 u64，初始为 0；每次打开和实际文档变化递增，无操作不递增。打开仅取 `sgf::Collection` 的第一盘，并建立新协调器隔离旧文档的节点编号和缓存；解析失败保留原文档。保存使用 `GameDocument::to_sgf` + `sgf::serialize`，同目录临时文件完整写入、flush/sync 后替换目标；保存不改变文档或 revision。
- `AppSubscription::recv()` 取消安全；watch 邮箱为文档、引擎状态、分析、整盘进度、棋盘同步状态各保留一个最新事件，按发布顺序读取，慢消费者的中间事件会合并且不会阻塞命令。文档变化、暂停实时分析、换引擎和关闭清除未读分析；发送分析前再次核对 revision/node/key。导航成功返回后，尚未读取的旧分析也不可再读。关闭或丢弃最后一个句柄会取消实时与整盘查询并等待进程回收，单独保留订阅不延长 actor 生命周期。
- 对外事件：`DocumentChanged { revision, view }`、`AnalysisUpdated(AnalysisView)`、`EngineStatusChanged(EngineStatus)`、`WholeGameProgress(WholeGameProgress)`、`BoardSyncStatusChanged(BoardSyncStatus)`（见 4.10）。
- 视图 DTO（前端只认这些）：
  - `BoardView`：宽高、每点棋子 `stones`、最后一手（含 pass）、轮走方、手数、黑白提子数。`stones` 与 `AnalysisView.ownership` 均为行优先、从左上角开始，下标 `i` 对应 `PointView { x: i % width, y: i / width }`；`y = 0` 是最上面一行（SGF 的 a 行，19 路为 GTP 第 19 行，矩形棋盘为第 height 行）。数组不翻转、不转置，非正方形棋盘也按 width 计算行跨度。
  - `TreeView`：root/current 节点 ID、含根和空节点的第一子节点主线、当前节点直接子节点数（包含主线），以及整树结构版本 `structure_revision`。`DocumentView.game_info: GameInfoView` 提供 black_name、black_rank、white_name、white_rank、komi、rules、handicap、result、date、event，遵循 document 的根信息语义，不因焦点改变。`DocumentView` 还含当前注释、由 `warnings()` 映射的结构化诊断。非法记录仍可导航，board 为 None，position_error 保留原因；协调器暂停直到回到可分析局面，不伪造棋盘。
  - 整树采用版本号加按需查询：`AppCore::variation_tree()` / Tauri `variation_tree()` / TS `variationTree()` 返回 `VariationTreeView { revision, root, nodes }`；每个 `VariationNodeView { id, parent, children, move_number, mv, move_error }` 可直接绘制，无需在 TS 解析 SGF 或推算颜色。actor 在发布文档时以 O(节点和属性数) 遍历、比较结构并缓存视图，节点增删、子节点换序及其撤销/重做才更新结构版本，导航、注释、分析更新不更新；成功打开棋谱总是更新，解析失败保留旧版本。版本取自该次应用 revision，在应用生命周期内单调增加，初始为 0。DocumentChanged 不携带整树；界面在 structure_revision 变化或首次连接时读取整树，只采用与最新 structure_revision 相等的回复。结构版本不包含 current，焦点高亮始终来自 DocumentView.tree.current。重连和慢消费者可直接取得最新全量树，不依赖接收过某次增量事件。
  - `AppCore::preview_variation(node_id, moves)` / Tauri `preview_variation(nodeId, moves)` / TS `previewVariation(nodeId, moves)` 返回 `VariationPreviewView { revision, node_id, board, applied_moves, stones, stop_reason }`。输入使用现有 MoveView（Rust 核心为 go-core::Move），通常直接传候选点 pv；可指定任意活动节点而不导航。board 是合法前缀执行后的 BoardView，move_number 是累计对局手数；stones 的 move_number 是从 1 起算的 PV 序号，按合法 PV 顺序列出最终存活的预览子。revision 是执行时的文档版本；界面须同时核对请求时的文档版本、节点和当前悬停请求，丢弃过时回复。命令仅通过 actor 串行读取文档，不发布事件、不修改撤销/重做历史、不调用协调器或触发分析；错误基准节点使命令失败，错误 PV 则返回部分结果及首个错误文字。
  - `DocumentView.graph: GraphView { points, current_index }`：显示线从根经过当前节点，再沿当前节点的第一子节点走到底；包含根、空节点和摆子节点，`current_index` 是当前节点在 `points` 中的零起算下标。每项 `GraphPointView { node_id, move_number, summary }` 的手数与 `BoardView.move_number` 一致：累计根起的着手（含根节点着手和 pass），注释、摆子、PL 不计手也不重置。非法记录仍保留显示线，含 B/W 属性的节点按一手记录计数，不表示着手合法。
  - `AnalysisSummary { winrate, score_lead, visits }` 保留黑方根评估。摘要表由 AppCore actor 独占，以节点 ID 索引；实时结果在 revision/node/key 校验、坐标转换成功并转发 `AnalysisUpdated` 时更新，整盘 Final 在节点/key 校验和坐标转换成功后同样合入。未分析节点为 null，同节点只保留最大 visits，相等时采用后来接受的值；摘要自身不限制分析转发。整盘结果每次合入后发布同一 revision 的 `DocumentChanged`，使图随进度填满，焦点和棋盘不变。成功打开新棋谱清空摘要，打开失败保留；导航、注释、暂停和删除/撤销保留历史摘要（墓碑不显示，节点 ID 不复用）。摘要与引擎指纹绑定：更换档案、原路径文件元数据变化导致指纹改变时清空；停用引擎或新档案无法取得有效指纹时也清空。同指纹重新配置/恢复不清空。清空非空表时立即发布同一 revision 的 DocumentChanged，使界面图数据同步重置，然后才发送新引擎的分析。
  - `AnalysisView.summary: AnalysisSummary` 表示合并本次接受结果后的该节点最佳摘要，不用于推算候选点的显示下限。P1-23 的文档保留结果独立于 LRU，重访时通过 `focus_with_cached` 发布完整根评估、候选点及 PV，该缓存快照按协调器原有规则建立 visits 下限；引擎重启或修改实时设置仍遵循 4.6 的下限重置规则，此时当前结果可以比最佳摘要浅。前端收到 `DocumentChanged` 时整体替换图数据，收到 `AnalysisUpdated` 时仅按 `node_id` 用事件的 `summary` 覆盖线上对应项，不比较 visits、不自行积攒跨文档摘要或判断换谱。
  - `AnalysisView`：node_id、revision、当前结果的根节点胜率 / 目差 / visits、实时速度 `visits_per_second: Option<f64>`、最佳摘要 summary、候选点、可选所有权数组 `ownership`、is_during_search、Cache/Live 来源。`CandidateView` 保留显示用 GTP `coordinate`、visits、胜率、目差、先验、顺序，并提供 `point: Option<PointView>`（pass 为 None）与 `pv: Vec<MoveView>`。PV 每手带颜色和格点（或 pass），从当前候选手的轮走方开始逐手交替，pass 也切换颜色。Rust 使用 go-core 已有的 GTP 转换；TS 直接绘制 DTO，不解析坐标或推导围棋颜色。数值和 ownership 从协调器原样保留，不二次翻转。
  - `AnalysisView::from_snapshot(snapshot, size, to_play)` 返回 `Result<AnalysisView, go_core::CoordinateError>`，先用本次根评估初始化 summary，actor 在实际转发时合并历史最佳摘要。actor 在发送 Focus 时保留对应的棋盘尺寸、轮走方，确认输出的 revision/node/key 匹配后才用于转换；非法候选点或 PV 坐标报告错误，不伪造 pass 或格点。9×13 构造响应测试将 `stones`、`ownership`、候选点和 PV 的同一交叉点逐项对齐。
- `AnalysisView.visits_per_second` 从协调器原样转发，JSON 始终包含数值或 null；为兼容现有界面构造的旧快照，ts-rs 生成 `visits_per_second?: number | null`，显示端按空值处理缺省字段。无足够采样、缓存和整盘读取均为 null，不在 TypeScript 计算速度。
- DTO 使用 `serde` + `ts-rs::TS`；带内容的事件/状态枚举以 `type`/`data` 判别，颜色为 Black/White，u64 revision/visits 的 TypeScript 类型为 number，与 JSON 数字一致。Tauri 层通过 `cargo run -p lizzieyzy-desktop --bin export-bindings` 递归导出事件、视图、落子参数、引擎档案及设置到 `apps/desktop/src/api/generated/`；一致性测试在临时目录重新生成，比较文件集合和内容，类型改动后须重新生成。
- 整盘命令由独立 `app-core::review` 和 `actor::review` 模块提供：`start_whole_game_analysis(WholeGameOptions)`、`pause_whole_game_analysis()`、`resume_whole_game_analysis()`、`cancel_whole_game_analysis()` 返回已应用命令后的 `WholeGameProgress`，不等待全部搜索完成；`whole_game_progress()` 可随时读取最新状态。`WholeGameOptions { visits }` 默认 1000，范围 500..=1,000,000；`WholeGamePreset::{Quick, Standard, Deep, Professional}` 分别对应 500、1000、3000、10000。非法选项或非法待分析局面在替换旧任务前报错。
- 开始时固定当前显示线：根到当前节点，再沿第一子节点到变化末尾，包含根、空节点、pass 和摆子节点；导航不会把任务切换成另一条线。每次提交或继续均按节点/key 检查保留结果，visits 不少于本次目标的直接计为已完成且不提交。暂停 terminate 剩余查询并保留结果，继续只补未完成节点；取消保留已完成结果并停止进度增长。`WholeGameProgress { completed, total, visits, status }` 的 completed 包括跳过的节点，status 为 Running / Paused / Completed / Cancelled / Error { reason }；完成时也取消仍多余的后台查询。已固定的节点被删除或局面改变后报 Error；打开别的棋谱、停用或切换引擎时取消任务，解析失败不取消原任务。
- `StoredAnalysis { node_id, key, response: AnalysisResponse }` 由 app actor 按节点保存在当前文档内存中，独立于协调器 LRU，保留完整 rootInfo、moveInfos（含每个候选点的 PV）及其余已解析统计。通过校验并转发的实时结果（包括 Partial）和整盘 Final 均可更新该表，同 key 只取更大 visits，相等时采用后来接受的值。P1-24/P1-25 通过 Rust `AppCore::whole_game_analysis(node_id)` 读取原始完整结果；`whole_game_result(node_id)` 提供同一结果的 `AnalysisView`，revision 使用读取时的应用版本，缺失、已删除或 key 不符的节点返回 None。打开新棋谱或引擎指纹变化清空；打开失败、同指纹恢复、导航、注释、暂停与取消保留。关闭实时分析仍可在导航时显示有效 Cache，Live 输出仍受 pondering 控制。
- Tauri 的 `commands/review.rs` 薄转发六个命令：上述四个控制命令及 `whole_game_progress`、`whole_game_result`，仅授权本地 main 窗口。TS 的 `api/review.ts` 提供对应 camelCase 封装，DTO 全部由 ts-rs 生成。生成的 `AppEvent`（导出为 `WireAppEvent`）包含整盘进度及其他领域事件；为保持本卡之外现有界面兼容，`backend.ts` 对原有 `subscribe` 暴露的 `AppEvent` 类型排除 `WholeGameProgress`，新增 `subscribeWholeGameProgress(listener)` 单独提供进度，仍共享同一个有序 IPC Channel 和 unsubscribe 生命周期。调用方先订阅再读取 `wholeGameProgress()` 初始化；mock-backend 只提供脚本指定进度和结果，不在 TS 实现整盘分析。
- P1-24 问题手由独立 `app-core::problem_moves` / `actor::problem_moves` 模块提供：`AppCore::problem_moves(ProblemMovesQuery)` / Tauri `problem_moves(query)` / TS `problemMoves(query?)` 返回 `ProblemMovesView { revision, settings, query, entries, error_bar, current_index }`。查询在 app actor 中读取与 `whole_game_analysis(node_id)` 相同的 P1-23 保留表，按当前节点到根再沿第一子节点到末尾构建显示线；不沿后台任务固定线，不修改文档、焦点、撤销栈、查询进度或启动分析。
- `MoveLoss { winrate_loss, score_loss, rank, is_best, candidate_rank, visits, before_visits }` 使用落子前父节点中 `order` 最小的候选点评估减去落子后节点的根评估；黑方取前减后、白方取后减前。胜率从概率转成百分点，目差单位为目，改善分别截为 0；首选手两项损失强制为 0。这些字段是落子方的损失，原始局面评估仍为黑方视角。`candidate_rank` 为引擎 `order + 1`，未命中为 null；pass 同样匹配。`visits` 为落子后根 visits（与 Java 列表一致），`before_visits` 保留父节点根 visits。
- 仅在前后节点保留结果均匹配当前 AnalysisKey、根 visits 和最佳候选 visits 大于 0、评估数值有效时计算损失。没有父节点的根着手、缺分析、非法记录、落子同时发生摆子/实际轮走重置、或落子颜色与父局面轮走方不一致时为 null，不将不同局面的变化归因于这手。普通首手可使用根节点的分析；空节点不计手，后续着手使用紧邻父节点的结果。停用/切换引擎、换谱时沿用 P1-23 失效规则，删除节点不显示，撤销恢复后仍须校验 key。
- `MoveRankSettings { mode, winrate_thresholds: [f64;5], score_thresholds: [f64;5] }` 默认阈值分别为 `[1,3,6,12,24]` 个百分点和 `[0.5,1.5,3,6,12]` 目，包含等号，从重到轻判定，产生 `Best/Good/Normal/Inaccuracy/Mistake/Blunder` 六档（严重度 0..5）。遵循当前 Java `MoveRankDefinition` / `MoveRankEvaluationMode` 的四模式：Winrate 只看胜率，Score 只看目差，Combined 取更严重的结果，默认 Auto 以目差为主、仅在胜率达到 Mistake/Blunder 时兜底；无目差时 Auto 回退胜率、Score 为 Best。KataGo 完整结果必有目差；纯评级接口保留 Java 的可缺目差语义。
- `ProblemMovesQuery { minimum_rank, color, sort }` 默认 Inaccuracy、双方、LossDesc。`entries: ProblemMoveView[]` 只含已计算、非首选且达到最低等级和颜色条件的手，每项给出 node_id、move_number、color、GTP coordinate、完整 loss。遵循 Java `ProblemListSort`：LossDesc 按胜率损失降序、相同损失按手数升序，独立于评级模式；另支持 MoveAsc/MoveDesc。任务卡的等级筛选替代 Java 旧面板的胜率门槛；API 不继承旧面板默认仅黑方或跳过首手的展示限制。`error_bar: MistakeBarPoint[]` 与 `GraphView.points` 严格同线同索引，含 node_id、move_number、mv 和可空 loss；根/空节点保留占位，已分析的首选显示零损失，列表筛选不裁剪失误条。
- `AppCore::set_move_rank_settings(settings)` / Tauri `set_move_rank_settings(settings)` / TS `setMoveRankSettings(settings)` 验证两个阈值数组均为正、有限且严格递增，胜率阈值不超过 100。先原子保存设置再生效，失败保留旧设置；成功变化发布同一 revision 的 DocumentChanged，重读立即按新阈值评级，无需重新搜索。界面先订阅现有 DocumentChanged、AnalysisUpdated 和 WholeGameProgress，再读取问题手；这些事件也覆盖整盘每次结果合入、导航、换谱和引擎失效。刷新须核对最新请求序号及文档 revision（分析/设置更新不增加 revision）；回复回显查询和实际设置。两个新命令只授权本地 main 窗口，DTO 由 ts-rs 生成，`api/problem-moves.ts` 与 mock 只转发命令或脚本数据。
- 设置保存在调用方目录下的 `settings.json`，`Settings { version, engine: Option<EngineProfile>, pondering, fox_history: Vec<FoxAccount>, move_rank: MoveRankSettings }` 当前版本仍为 1。缺字段分别默认（旧文件的野狐历史为空列表，问题手使用默认阈值和 Auto 模式；只有其中一类字段的文件也补齐另一类）、多余字段忽略；版本 0/1 读入后规范为 1，未来版本、损坏 JSON 和无效评级阈值报错，避免覆写未知或不可用格式。P1 保存一个活动档案，`None` 关闭引擎；配置、暂停状态和评级修改成功时持久化。设置保存同样使用临时文件替换。
- `EngineProfile { program, model, config, extra_args, canonical_model_name: Option<String> }` 的指纹为程序/模型/配置的路径、普通文件大小、修改时间（秒及纳秒）和额外参数的版本化 JSON 编码的 BLAKE3；P1-23 使用前缀 `lizzieyzy-next2:engine-profile:v2-two-search-slots` 标识强制双搜索槽的启动语义。它是缓存身份，不是文件内容校验和。`canonical_model_name` 是经目录 SHA-256 识别的规范模型文件名，仅供展示，不参与指纹；旧档案缺少此字段时为 None，未识别的模型同样为 None。自动配置可将实际路径 `weights/default.bin.gz` 标记为 `kata1-tf3-b11c768-s12002M-d6304M.bin.gz`，界面可据此展示模型系列。改变档案或重新配置时发现元数据变化，会先关闭旧协调器和会话，再创建新的；同一有效档案幂等，失败会话允许重新配置重试。文件不存在发出包含用途和路径的 Failed，文档编辑仍正常。
- **数值一律黑方视角**：应用自带的 `resources/katago/analysis.cfg` 显式设置 `reportAnalysisWinratesAs = BLACK`。`TransportSpec::LocalProcess` 启动参数为 `analysis -config <配置> -model <模型> -override-config <合并值>`，整个命令中 `-override-config` 只出现一次，以符合 KataGo 的 TCLAP ValueArg 约束。档案额外参数里的各个 `-override-config VALUE` / `-override-config=VALUE` 被提取并按原顺序合并为逗号列表，显式 BLACK 和 numAnalysisThreads 项去重，末尾追加唯一 `numAnalysisThreads=2,reportAnalysisWinratesAs=BLACK`；其它非 override 参数保留边界及相对次序。额外参数仍不得覆写模型/配置路径，也不得把视角改成非 BLACK。
- 双搜索槽用于让持续实时请求与整盘批次共存；配置文件和额外 override 中的 numAnalysisThreads 均由启动参数固定为 2，不改写原配置文件。优先级只决定排队顺序，不能抢占已开始的搜索：实际焦点 node/key 改变，或重新开启实时分析时，app actor 先 terminate 整盘剩余查询，提交新实时请求，再按保留结果重建剩余批次，防止后台曾占满两槽时阻塞新焦点。已接收完成的节点不重算；仅 revision/注释变化不打断查询。具体吞吐和交互延迟仍取决于实际模型、GPU 与每位置搜索线程配置，fake-katago 测试验证调度与持续事件，不代替真实 GPU 性能验收。
- 首次设置使用 `EngineSetupProfile { program, model, config: Option<PathBuf>, extra_args }` 和 `AppCore::try_configure_engine(profile, default_config_source)`；旧 `EngineProfile` 档案仍可读取，`configure_engine` 的先存后启语义不变。`config=None` 时从调用方指定的模板复制到设置目录的 `analysis.cfg`，用同目录临时文件原子发布且不覆盖已有普通文件；拒绝目标为目录或符号链接。失败尝试可能留下已复制的默认配置，但不改变 `settings.json` 或活动档案。Tauri 将 `resources/katago/analysis.cfg` 映射为 `katago/analysis.cfg` 资源，从 `resource_dir` 解析来源；app-core 不依赖 Tauri。
- 试启动复用 `EngineProfile::transport` 的参数校验、单次 override 合并与指纹计算。候选 engine-session 独立运行，由 app actor 的待定事务持有；通过 `status()` 的 watch 事件等待 Ready，期间旧引擎继续输出，文档和设置命令继续处理。Ready 后保存设置并直接接管已试好的会话，不重新启动一次。Failed、文件缺失、保存失败或超时以命令错误返回，不发布候选失败为全局引擎状态，不切换旧会话。`AppConfig.engine_setup_timeout` 默认 120 秒、允许 `(0,24h]`，只作兜底；新试启动或直接配置取消旧试启动，关闭/丢弃应用句柄也回收候选进程。
- `AppCore::auto_configure_engine(roots: SearchRoots, default_config_source: PathBuf) -> Result<EngineProfile, AppError>` 是 P1-28 自动配置入口。已经保存引擎档案时原样返回，不探测、不试启动、不重存设置；否则在 `spawn_blocking` 中探测并按模型排序逐个尝试，每个模型先配同一安装的程序，再配其他程序，组内保持程序发现顺序。路径 canonical 去重仍保留安装关联，包括先从 PATH 发现、后从安装根发现同一程序的情况。自动配置统一使用新版自带分析模板，不选择 Java 安装中的 GTP 配置；沿用已有 `analysis.cfg` 不覆盖语义。
- 自动配置先执行候选程序的 `version` 子命令，每个程序的结果在本次调用内复用；版本探测最多 3 秒，stdout/stderr 各限 16 KiB，失败、不可解析版本或低于已识别模型的 `minimumKataGoVersion` 均跳过该组合。最多检查 6 对组合（包括版本跳过和失败），每次分析试启动等待 Ready 最多 20 秒；探测、版本检查、准备和等待 Ready 共用 120 秒预算，到期不再开始提交。候选禁用自动重启，候选进程关闭宽限至多 2 秒。试启动仍通过 `engine_trial` 和 app actor 的 Ready 事务，成功后原子保存、接管该会话并开始分析；一旦开始原子保存，就等待保存和接管完成再回复，避免调用方收到超时失败后配置已经落盘。准备和指纹检查受提交前 deadline 限制，未开始提交的失败保留原设置；非常慢的文件系统保存和操作系统进程回收不保证绝对墙钟硬上限。新的自动请求或手动配置取消旧的待定自动事务，关闭/丢弃应用句柄回收候选进程。
- 自动配置的类型化错误为 `AutoSetupError::{NoPrograms, NoModels, AllFailed { attempts, reason }}`；每个 `AutoSetupAttempt { program, model, reason }` 保留已检查组合的失败或跳过原因，`reason` 还说明全部失败、预算耗尽或取消等终止原因。`AppError::AutoSetup` 经 `CommandError::AutoSetup` 原样传递给界面，错误及尝试 DTO 由 ts-rs 生成。
- `engine_setup::detect(&SearchRoots) -> EngineDiscovery { programs, models, configs }` 只查调用方注入的目录，不读取环境、不执行程序。`SearchRoots` 接收 PATH 目录、Homebrew 前缀、应用目录、Windows KataGo 安装目录，以及 `resource_dirs`、`java_install_dirs`、`platform_dirs`。PATH 查 `katago`/`katago.exe`；Homebrew 查 `bin/katago` 与 `share/katago`；应用目录查旁边的 `katago/`；旧安装根只扫描自身、`bin` 程序和 `models/networks/config/configs` 模型配置。新增资源与 Java 根均为直接包含 `engines/`、`weights/` 的内容目录，只查 `engines/katago/<platform>/katago[.exe]`、`weights/` 和 `engines/katago/configs/`，不枚举其他平台或递归全盘。识别模型 `.bin.gz/.txt.gz/.bin` 和配置 `.cfg`，忽略不存在或不可读目录，Unix 程序须有可执行位，按 canonical 路径去重并保留首先发现的路径。
- `resources/katago/katago-assets.json` 逐字节来自 Java 仓库提交 `993e288` 的 `src/main/resources/katago-assets.json`。本功能只读取 `defaultModelId`、`models` 及模型的 `fileName/sizeBytes/sha256/tier/minimumKataGoVersion`，不使用下载地址或引擎 assets。只有本地文件大小命中任一 `sizeBytes` 才流式计算 SHA-256；名称不参与身份判断，因此改名为 `default.bin.gz` 仍能识别。命中目录的模型整体优先，默认模型 B11 第一，其余依次 strongest、balanced、lightweight。未识别模型沿用文件名规则：`kata1-*`、`g170*`、其他族；族内按 `-s` 训练计数、`-d` 数据计数降序，再按小写文件名和路径排序。计数支持大小写 M/G 十进制后缀（`s12002M` 为 12,002,000,000），缺失或溢出视为 0。程序保留搜索根优先级，配置按路径排序；推荐排序不代表已验证硬件兼容性。
- 壳层继续提供当前 PATH、macOS `/opt/homebrew` 与 `/usr/local`、可执行文件目录（macOS 也查 `.app` 旁边，Linux AppImage 也查原始安装文件旁边）；Windows KataGo 位置仍为 `C:\KataGo`、ProgramFiles / ProgramFiles(x86) / LOCALAPPDATA 下的 KataGo、用户目录下 KataGo 及 Scoop 的 `apps/katago/current`。新版资源直接使用 Tauri `resource_dir`，平台目录按当前系统和架构显式选择 `macos-arm64`、`macos-amd64`、`windows-x64` 或 `linux-x64`，无对应平台时不尝试其他架构。
- Java 固定根依据 `package_macos_dmg.sh`、`package_windows_exe.sh` 和 `package_release.sh`：macOS 查 `/Applications`、`~/Applications` 及新版应用邻近位置的 `LizzieYzy Next.app/Contents/app`；Windows 查 ProgramFiles / ProgramFiles(x86) 及新版应用邻近位置下 Java 包的固定名称目录（含 OpenCL、NVIDIA、TensorRT 等分包名）中的 `app/`，也查新版可执行目录旁的 `app/`。Linux 包是 portable `Lizzieyzy/`，脚本没有规定默认安装位置，因此保守约定只查 `/opt/Lizzieyzy`、`~/Lizzieyzy` 和新版应用旁的 `Lizzieyzy/`，任意解压位置不会自动全盘查找。所有测试注入假根，不依赖这些位置实际安装了 Java 版。
- Tauri 层：`commands/{document,variation,analysis,engine,settings}.rs` 的业务命令只做参数转换和调用 `AppCore`。文档入口为 `open_bytes`、`open_path`、`save`、`play`、`step`、`go_to`、`undo`、`redo`；只读展示入口为 `variation_tree`、`preview_variation`；分析和设置入口为 `set_pondering`、`configure_engine`、`settings`。`play` 及预览着手接受现有 `MoveView` 后转换；壳层不依赖 document / go-core，不复制其业务规则。
- 首次设置的 Tauri 命令为 `detect_engines() -> EngineDiscovery`、`try_configure_engine(profile: EngineSetupProfile) -> ()` 和 `auto_configure_engine() -> EngineProfile`，只授权本地 main 窗口。探测在 blocking 任务执行，根目录与模板来源由壳层托管；测试注入临时目录。前端 `detectEngines` / `tryConfigureEngine` / `autoConfigureEngine` 是薄 IPC 封装，新增 DTO 同样经 export-bindings 生成；E2E mock 仅返回脚本指定结果，不实现探测或试启动业务。自动配置接口本身不打开设置窗口；首次启动调用、失败后展示设置窗口和规范模型名称展示属于 `App.tsx` / `settings/` / `locales/` 的后续界面接线，不能仅凭此后端接口验收首次启动的完整界面行为。
- `subscribe` / `unsubscribe` 管理传输订阅。五种 `AppEvent` 共用一个 `tauri::ipc::Channel<AppEvent>`，由单个转发任务按 `AppSubscription::recv()` 的顺序发送；不分事件种类并发转发，确保 revision N 的 `DocumentChanged` 在其 `AnalysisUpdated` 之前。转发任务保留最近的文档、引擎状态、整盘进度和棋盘同步状态四条事件，按原收取顺序回放给新订阅，不缓存或回放分析；分析的有效性仍由 AppCore 决定。没有活跃 Channel 时暂停读取 `AppSubscription`，由 AppCore 的有界邮箱保留最新有效事件并清除过期分析；首次订阅前或断连期间产生的有效最终结果不会被壳层提前消费丢弃。仅有一个活跃 Channel，新订阅替换旧订阅；订阅 ID 是单调递增的 u32，耗尽时报错，过期 ID 的取消操作幂等且不影响新订阅。前端 `src/api/backend.ts` 是统一业务入口，封装命令和订阅生命周期；多个界面监听者共享同一传输订阅。
- 启动默认使用 Tauri 的应用配置目录，可用 `--config-dir <路径>` 或 `--config-dir=<路径>` 覆盖；棋谱路径可用一个位置参数或 `--open=<路径>` 指定，交给 `AppCore::open_path`。等号写法将选项和路径放在同一个原生参数中，兼容将每项参数都视为 Chromium 开关的 Windows WebDriver；桌面 E2E 在所有平台统一采用等号写法。参数顺序不受限，`--` 后的参数仅作棋谱路径，不再识别选项；未知选项、重复配置目录、多个棋谱和缺少或空路径均报错，混用两种写法仍按同一配置目录／棋谱参数检查重复。路径保持原生 OS 编码；启动打开失败时等待 AppCore 关闭后返回错误，不留下后台引擎。
- 主窗口 capability 仅匹配本地 `main` 窗口，逐项允许应用命令、`dialog:allow-open` / `dialog:allow-save`，以及「文件 → 退出」关闭自身用的 `core:window:allow-close`；不授权远程页面或宽泛 core / shell / fs 权限。`tauri-build::AppManifest::commands` 为应用命令生成 ACL 定义，确保命令同样受 capability 约束；生成的 permissions 与 schemas 不入库。Channel 内部的数据提取命令由 Tauri 自身处理，不需要额外的 core capability。

### 4.8 strength（棋力评估）

- `strength::Estimator::bundled()` 从编译时嵌入的模型 JSON 构造纯同步估计器，无网络、文件 I/O 或异步运行时。`estimate(&[MoveInput])` 返回黑、白、整体 `StrengthReport`；每手输入包括手数、颜色、坐标、前后根评估的损失及落子前按 order 排序的候选点；app-core 先通过 P1-24 每手损失入口检查有效性。候选点的胜率由 app-core 从黑方概率转换为落子方百分点，目差同样转换到落子方；这只是计算差值的内部输入，对外局面分析仍为黑方视角。
- `Sample::from_input` 复现 Java `PlayerStrengthEstimator`：命中候选时优先使用同一父局面首选与实际候选的差值；严格首选、前三、前五按候选列表位置计数，未命中为 999；复杂度按 prior 加权的候选损失计算（排除 order >= 999 的追加候选），无 prior 时回退次选差。损失取非负，复杂度限制到 0..1，难度权重为 `clamp(max(complexity, score_equivalent_loss/4), 0.05, 1)`；无目差时以胜率百分点损失除以 6 换算。未命中候选时按 Java 使用前后根评估差；P1-24 既有问题手采用首选候选到落子后根评估，两者口径分别保留，不修改问题手结果。前后分析缺失、key 不符、非法记录、摆子或轮走重置等均沿用 P1-24 的排除规则。
- Java 的 29 项特征按固定 20 索引输入 XGBoost，各树从根沿 `<` 分支推理，缺失特征走 `default_left`，累加 base score 后限制到 [-1,12]，再应用配套残差校准。模型文件逐字节来自 Java `src/main/resources/models/strength/`；来源、SHA-256 存在 `crates/strength/models/provenance.json`，单元测试核对哈希。`format_rank` 提供 Java 对应的中英文级、段、职业、一线职业与 AI 格式；等级尺计算也位于 Rust。
- `SideReport` 返回原始 29 项特征、校准段位、中英文段位文字 `rank_label_zh/en`、固定预测 sigma、样本、严格首选率、前三/前五率、Java 加权吻合度、平均胜率/目差损失、加权损失、失误率、阶段统计和可信度。阶段按整盘手数分为 1..60、61..160、161 以后；无该阶段样本时，特征用的阶段加权损失和好手率回退整体值，并同时返回零样本数。显示等级使用现有 `MoveRankSettings::classify_losses` 对棋力评估实际采用的损失重新分类；`tier_counts` 只把严格首选放入 Best，其余 Best 归 Good；显示评级好手率仅统计 Best/Good，与按目损失划分的 `good_move_rate` 分别保留。
- 整体不足 6 个有效样本时 `has_enough_data=false`；有少量样本仍保留 Java 的低可信估计。单方样本少于 16 为 Low，至少 16 为 Medium，至少 40 且 70% 有目差为 High。完全没有样本的段位及 sigma 为 null，缺少目差的均值/中位数为 null，避免把缺失当作零损失。`total_moves` / `analyzed_moves` 单独公开覆盖率，不把局部分析声称为整盘完成。
- `AppCore::strength_estimate()` / Tauri `strength_estimate()` / TS `strengthEstimate()` 返回 `StrengthEstimateView { revision, report, total_moves, analyzed_moves }`。读取与图和问题手相同的当前显示线，复用 P1-23 保留结果和 P1-24 有效性检查；不导航、不编辑、不启动分析。app actor 仅缓存不可变模型，每次查询从当前保留结果重算报告，因此整盘/实时结果、分支切换、换谱、换引擎后立即反映变化，不维护另一份过期报告缓存。调用方先订阅 `DocumentChanged`、`AnalysisUpdated`、`WholeGameProgress` 再读取，按最新请求序号及 revision 丢弃旧回复（分析更新不增加 revision）。命令仅授权本地 main 窗口；全部 DTO 经 ts-rs 生成，mock 只返回脚本数据；界面和 HumanSL 不属于此接口实现范围。

### 4.9 野狐公开棋谱

- `fox-kifu` 不依赖其他工作区 crate，使用 reqwest 的 rustls 后端访问公开 HTTP 接口，不需要账号登录、Cookie 或外部 JAR。`FoxClient::search(&FoxQuery)` 返回 `FoxSearchResult { account, page }`；`FoxQuery::{Nickname(String), Uid(String)}` 在 IPC 中使用 `type/value` 判别，数字昵称不会被自动解释为 UID。UID 只接受 ASCII 数字。
- `FoxAccount { uid, nickname }` 保留账号身份；`FoxPage { games, next_cursor }` 的每项 `FoxGame` 包含对局 id、开始时间、黑白 `FoxPlayer { uid, nickname, rank }`、结果和手数。结果由胜方、胜负方式、可选分值及子/目单位组成，区分中盘、超时、点数和其他胜负；显示文案留给界面本地化。段位沿用 Java 的数值偏移及职业 `P` 规则。
- `FoxClient::list(uid, cursor)` 使用服务端最后一项对局 id 翻页，每页上限 100；不足 100 项（包括空页）表示没有下一页，重复游标报错。`download(game_id)` 先用 okhttp UA 依次 POST 主、备用棋谱 CGI，再回退到 H5 GET；HTTP 错误、非 JSON、非零结果或空棋谱均继续下一接口。
- HTTP 使用独立的 20 秒连接超时、默认 25 秒读取空闲超时；持续收到数据会重置读取期限，慢响应不会因总传输超过 25 秒而失败。每次操作（含重试和退避）另有 3 分钟上限、每份响应最多 4 MiB。只有超时、连接和正文传输错误最多尝试 3 次，重试前依次等待 350/700ms；HTTP 状态、JSON、API 和大小错误不重试。`with_endpoints` 的 Duration 参数仅覆盖读取空闲超时，供本地 HTTP 测试注入。
- 下载成功后删除 BOM 和 SGF 属性值外的反斜杠，再按 Java 的确定性规则选择首个含着手的变化、合并根信息和整理着手为主线。保留所选主线的注释、未知属性和值内转义，不依赖全局棋盘去推测拼接窗口碎片。原始 `KM` 保持不变，贴目解释仍由 document 执行；例如 `AP[foxwq]`、中国规则的 `KM[375]` 打开后是 7.5 目。损坏结构返回类型化错误供接口回退。
- 所有失败返回 `FoxError`；`is_retryable_transport()` 是跨平台的网络重试判定，连接失败、连接/读取超时和正文中断均属于这一类，不依赖系统错误码。保留 `Timeout` 与 `Transport` 诊断区别；reqwest 在连接完成前也检查读取期限，因此短期限下 Windows 的连接拒绝可能表现为 `Timeout`。网络错误的 Display 使用可读文案，底层细节只保留在错误源/Debug 中。确认的无账号响应（101201 且 UID 空或 0）与 HTTP、网络、其他 API 或格式错误分开，未知非零码保守作为 API 错误。
- `AppCore::{fox_search, fox_next_page, fox_open_game, fox_clear_history}` 是壳层唯一入口。调用 future 执行 HTTP，应用 actor 只处理客户端取得、历史持久化和下载完成后的 `open_bytes`，慢网络不会占住文档/设置命令。分页参数显式传入，不共享可覆盖的全局游标；下载后沿用本地 SGF 的打开事务、事件及焦点更新流程，失败保留当前文档。
- 成功取得账号和第一页后，才把 `FoxAccount` 记入 `Settings.fox_history`，按 UID 去重、最近成功完成的搜索在前、最多 10 条；加载设置也规范化历史，清空通过同一原子保存流程持久化。`AppConfig.fox_client` 可注入本地测试端点，默认客户端惰性创建。
- Tauri 的 `commands/fox.rs` 提供 `fox_search(query)`、`fox_next_page(uid, cursor)`、`fox_open_game(gameId)`、`fox_clear_history()`，仅授权本地 main 窗口。`CommandError::Fox(FoxFailure)` 保留可本地化的 `Network`（网络错误）、`Timeout`（网络超时）分类及 HTTP/API 状态码；下载回退耗尽时若包含网络原因，仍返回网络分类（优先 Network，其次 Timeout），纯协议失败才返回 DownloadFailed。IPC 只传类型和数据，不暴露平台错误文本；最终提示由界面本地化。其余错误维持原有应用错误边界。所有野狐 DTO 和错误类型由 Rust 递归生成 TS，前端 backend 与 E2E mock 仅转发或回放命令，不实现网络协议或解析。
- 普通测试使用 `fixtures/fox/` 的真实录制和本地假 HTTP 服务；列表裁为 5 盘，昵称、UID 及 SGF 的 PB/PW 一致脱敏，保留字段类型和业务结果，清理敏感字段。录制日期和接口在测试注释中注明。`fox-recorded-2026-10-30.sgf` 保留录制的 177 手棋谱；此外保留 Java 八手回归片段和 [Leela Zero #2285](https://github.com/leela-zero/leela-zero/issues/2285) 公开发布的 313 手完整野狐棋谱；SGF 样本由独立 oracle 校验。`app-core/tests/fox_live.rs` 的联网验收显式 `#[ignore]`，经生产 AppCore 完成昵称、列表、CGI/H5 下载和打开，报告列表盘数、对局 id、手数和贴目，仅使用临时目录，不把未脱敏网络响应写入仓库。

### 4.10 Windows 棋盘同步（readboard）

- 协议来源为 [qiyi71w/readboard](https://github.com/qiyi71w/readboard/tree/a292eea71caaf0cc2f087d3a7563e35db01684b4) 的 `Program.cs`、`MainForm.WebView.cs`、[LaunchOptions.cs](https://github.com/qiyi71w/readboard/blob/a292eea71caaf0cc2f087d3a7563e35db01684b4/readboard/Core/Models/LaunchOptions.cs)、[OutboundBoardSnapshotEmitter.cs](https://github.com/qiyi71w/readboard/blob/a292eea71caaf0cc2f087d3a7563e35db01684b4/readboard/Core/Protocol/OutboundBoardSnapshotEmitter.cs) 和 `SyncSessionCoordinator*`，以及只读 Java 参考仓库的 `analysis/ReadBoard.java`、`ReadBoardStream.java`、`SyncSnapshotClassifier`、`SyncSnapshotRebuildPolicy`、`SyncSnapshotDiffChecker`、`SyncRemoteContext`。以该上游提交的实际发包源码为准，不将旧 fixture 中没有逗号的简写当成当前协议。
- Windows 壳固定解析 `<resource_dir>/readboard/readboard.exe`，工作目录为其所在目录，不接受来自 IPC 的程序路径，不经过 shell。启动参数逐项为 `yzy`、单个空格、单个空格、单个空格、`0`、`cn` 或 `en`、`-1`；三个空格占位是禁用自动落子的时间/访问数配置，不能丢失参数边界。`0/-1` 选择管道方式。Java 还支持 `1/<port>` 的 TCP 回连方式，新版只实现 stdio，无监听端口。P1-34 负责捆绑 exe 及其运行依赖，本卡不下载、更新或打包识别器。
- 双向 UTF-8、LF/CRLF 分行。helper 发 `ready` 后宿主只发送一次 `version\n` 和 `analysisState running\n` 或 `analysisState paused\n`；收到 `version N` 可识别版本但不发自动更新能力。退出发送 `quit\n` 并关闭 stdin。写入与正常退出各有 2 秒期限，超时强杀后等待实际退出；启动后 15 秒没有 ready 报错。stdout 的部分行取消安全地保留，单行限 8192 字节，stderr 持续排空但不当协议解析，避免日志阻塞进程。
- `start W H [hwnd]` 设置尺寸并清空未完成帧；裸 `start` 保留尺寸（初始 19）。每个 `re=` 行是以逗号分隔的一整行棋盘，恰有 W 项；H 行后 `end` 才提交完整快照。编码 `0=空、1=黑、2=白、3=黑末手、4=白末手`，行优先、左上原点，与 BoardView 相同。`clear` 只丢弃半帧，`forceRebuild` 标记下一完整帧必须重建。尺寸、行列数、码值、多末手、坏元信息均验证，损坏帧不提交，后续 start/clear 可恢复；超长行、非法 UTF-8、意外 EOF 为进程级错误并回收进程。
- 帧元信息包括 `syncPlatform`、`roomToken`、`foxMoveNumber`、`liveTitleMove`、`recordCurrentMove`、`recordTotalMove`、`recordAtEnd`、`recordTitleFingerprint`、`lastMoveSource`，都在 end/clear/start 后清除，不能沿用前帧。可用的野狐手数要求平台 fox、明确 foxMoveNumber，且标题/棋谱手数没有冲突。房间、平台、棋谱身份变化不跨局拼接。`lastMoveSource` 区分真实末手标记与棋子数推断，后者不能决定轮走方。
- `sync` 开始持续同步；完整快照亦进入 Syncing。`stopsync` 回到 Ready，保留工具进程；`endsync` 正常结束工具会话并回收进程。`clearBoard` 清空文档但保持棋盘尺寸。`noponder/resumeponder` 暂停/恢复本次同步的分析，不改保存的设置。未知扩展消息忽略；`play/both/noboth/inboard/notinboard` 等自动落子/浮动棋盘控制不在本卡范围，`pass` 是 helper 的交换轮走按钮而非实际停一手，新版暂不执行，绝不伪造 SGF pass。
- `readboard::Decoder`、`Snapshot`、`Cell`、`RemoteContext` 和 `Command::encode` 都是纯同步类型，只依赖 go-core 与 thiserror；不持有 I/O 或线程。唯一 app-core actor 同时独占 readboard 子进程、解码器、同步状态及 GameDocument，通过 select 接收命令和完整行；无额外文档写入任务、锁或共享可变状态。停止命令、应用关闭、丢弃最后一个 AppCore 句柄都等待进程回收，订阅不会延长会话寿命。相同程序与选项重复开始为幂等；新程序启动失败保留原会话。成功打开另一棋谱会先结束同步，解析失败保留现有棋谱和会话。
- 局面整合先匹配同步末端；可信野狐手数还允许匹配主线已有历史节点。唯一新增且末手标记匹配、合法落子及提子后的全盘与快照完全一致时逐手追加，复用已有直接子节点；无标记仅允许可信野狐手数严格 +1 且仅增加一子的情形。无法接上时，无历史、尺寸/房间变化、明确 forceRebuild 或可信远端手数跳变直接重建；已有历史的其他冲突须连续两份相同快照确认，第一帧保持原文档。重建保留对局信息，以 AB/AW/PL 建立合法摆子根，保存可信远端 MN，不编造未看见的历史着手；零气等非法摆子拒绝且保留原文档。应用 revision 持续递增，重建重置节点命名空间、旧分析缓存及整盘任务。
- `BoardSyncOptions { always_sync, always_goto_last, language }` 默认 true、false、Cn，沿用 Java 两个独立选项。`always_sync` 对应「总是保持棋盘一致」：增量后推进同步末端；关闭它不禁用全盘差异校验、提子核验和冲突重建。Java 当前 `shouldResyncAfterIncrementalSync` 本身不使用该开关。`always_goto_last` 决定新手到来时是否跳离用户正在浏览的历史；默认 false 时恢复浏览节点，重复帧不把视图拉回。同步分析焦点独立指向已接受的远端节点：用户浏览其他节点时结果仅更新该远端节点的保留分析和图，不发可叠到错误盘面的 AnalysisUpdated。开始同步自动分析最新局面，停止恢复保存的 pondering 设置；没有已配置的引擎仍可同步棋谱，不伪造分析结果。
- `AppCore::start_board_sync(program, options)`、`stop_board_sync()`、`board_sync_status()` 返回 `BoardSyncStatus { phase, always_sync, always_goto_last, remote_move_number, error }`。phase 为 Stopped/Starting/Ready/Syncing/Failed；开始回复表示子进程已创建，ready/快照通过后续事件确认。远端手数与重建后本地已知手数独立；`error` 保留类型化 UnsupportedPlatform/MissingExecutable/InvalidOptions/Process/Protocol。缺 exe 在启动命令返回 MissingExecutable，不自动下载。Tauri 三个同名命令为薄适配，仅本地 main 窗口可用；非 Windows 开始命令返回 UnsupportedPlatform。没有 macOS/Linux 识别实现。
- `BoardSyncStatusChanged` 与既有事件共享有序 IPC Channel 和第五个有界最新状态槽，壳层重连回放状态；`subscribeBoardSyncStatus` 从同一连接分流，旧 `subscribe` 的事件类型排除此扩展。TS 的 `startBoardSync`、`stopBoardSync`、`boardSyncStatus` 只传命令，所有 DTO 由 export-bindings 生成；mock 仅回放脚本。菜单、状态展示、语言化错误提示和选项界面由后续界面工作接线，本卡没有修改这些部分。
- `fixtures/readboard/reconstructed-session.txt` 是依据上述固定提交的发包源码构造的回放样例，**不是 Windows 实机录制**；公开上游未提供可直接使用的完整现场 wire dump。golden 测试固定解码结果，property 测试检查往返及任意输入不会 panic。fake-readboard 是独立 std-only Cargo workspace，由测试使用本克隆 target 构建；以同样七个参数启动，收到 version 后回放工作目录 replay.txt，记录收到的命令并在退出前写标记。普通集成测试不依赖 Windows、真实识别器或 GPU；这些结果不能替代 P1-34 的 Windows 安装包与真实客户端验收。

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
| 全栈 | tauri-driver（Linux）/ Edge WebDriver 附加（Windows CI）+ fake-katago | 主流程 |

- Windows 原生 E2E 由测试进程用普通构建产物启动应用，再以 `browserName: webview2` 和 `ms:edgeOptions.debuggerAddress` 附加到 `127.0.0.1:9222`，不向驱动提交应用启动参数。启动前确认端口空闲，等待 CDP 端点就绪；每个场景结束后关闭会话并回收应用进程树。Linux 继续由 tauri-driver 启动和管理应用。
- GitHub Windows runner 的提权宿主会忽略 `WEBVIEW2_*` 环境覆盖，因此只有 Windows CI 包装脚本临时设置 HKLM WebView2 `AdditionalBrowserArguments` 策略，值名精确匹配 `lizzieyzy-desktop.exe`。原值、类型和存在性在修改前持久化，`finally` 恢复，工作流另以 `always()` 步骤幂等恢复；恢复错误使 CI 失败。普通启动诊断在策略设置前运行，使用默认设置。正式构建配置、应用代码和发布工作流均不包含调试端口，测试脚本不进入产品包。

### fake-katago 测试进程约定

- 启动形状为 `fake-katago analysis -config FILE -model FILE`；接受路径参数及单次 `-override-config KEY=VALUE[,KEY=VALUE…]`；支持的命令行参数重复出现时以退出码 2 报 `Argument already set`（即使值相同也拒绝），不读取模型或配置，也不调用真实 KataGo。输出明确标识版本为 `1.16.5-fake`。
- 单个调度器管理排队与运行中的搜索；并发位置数默认 1，`-override-config` 中的 numAnalysisThreads 可覆盖默认值，显式 `--threads` 为测试覆盖且不受参数顺序影响。优先级高的先开始，相同优先级按输入顺序；已开始的搜索不会被抢占。stdin 关闭后排空正常搜索再退出。`--request-log PATH` 可追加实际收到的协议行，供行为测试核对批量手数、优先级、terminate 及恢复时不重复完成节点；不用于生产引擎。
- 同一请求的 `analyzeTurns` 按手数去重、递增入队，重复手数采用最后一个对应优先级；`priority` 与 `priorities` 不得同时提供。只有 `firstReportDuringSearchAfter` 时仍输出一次首次中间报告。未指定初始玩家且无历史时，超过一颗初始黑子、没有白子则默认轮白，其余默认轮黑。
- 候选点及评估由棋盘尺寸、初始摆子、着手序列和手数的固定哈希生成，与请求 id 无关。默认 `maxVisits=100`、`--visits-per-sec=1000`；visits 随模拟时间增长，最终结果达到上限，取消则保留已有 visits。默认评估和 ownership 均为黑方视角；`--default-perspective-to-play` 模拟缺少视角配置时的轮走方视角。显式 `reportAnalysisWinratesAs=BLACK/WHITE/SIDETOMOVE` override 优先于该默认标志；在同一份逗号列表内，同一键重复赋值最后一个生效。除视角和 numAnalysisThreads 外，其它配置键仅接受而不模拟。
- `--time-scale` 表示每个真实秒经过多少模拟秒（默认 1，2 表示两倍速）；搜索、报告间隔、`--startup-delay-ms` 和 `--crash-after-ms` 都使用模拟时间。启动与定时崩溃从进程调度器初始化开始计时。
- `--crash-after-queries N` / `--hang-after-queries N` 在接受第 N 个有效分析请求时触发，动作不计数，0 表示启动即触发。崩溃退出码为 1；挂起后连版本回复、取消确认也不再输出，直到进程被终止或定时崩溃触发。
- `--fault-once-file PATH` 为查询崩溃／挂起、`--crash-after-ms` 定时崩溃和 `--fail-startup` 启动失败共用一个跨重启的一次性标记：仅在首次实际触发故障时原子创建文件，已有普通文件则跳过故障，文件错误明确报错。首次进程到达定时崩溃期限时创建标记并退出，重启后的进程跳过该故障并清除已到期的 deadline，避免忙循环；启动失败同样只触发一次。正常启动及尚未触发的故障不消耗标记。测试使用独立临时路径并负责清理。
- `--startup-gate-file PATH` 在外部创建普通文件后才允许处理 stdin（也须满足启动延迟），用于确保请求在 Ready 前全部入队。假引擎不创建或删除该文件；等待期间定时崩溃仍生效。
- `--fail-startup` 未被一次性标记跳过时，立即向 stderr 写入“模型找不到”并以退出码 1 退出；`--garbage-every N` 在每 N 条正常 stdout 协议行后额外插入一行坏数据，故意用于测试会话的坏行处理。
- 取消顺序遵循录制：排队结果先 `noResults`，然后动作确认，最后是运行中查询的最终结果；`terminate_all` 同时取消所有排队与运行中的位置。
- 假引擎检查坐标、已有棋子、提子、自杀和简单劫；不作为完整规则裁判，不模拟超级劫、真实搜索、模型设置或对称剪枝。录制对照比较公共字段与状态顺序；重复中间采样数量和条件性 `isSymmetryOf` 不要求与 GPU 搜索逐项相同。
- 测试库函数 `fake_katago::binary_path() -> Result<PathBuf, BinaryPathError>` 首次调用执行 `cargo build -p fake-katago --bin fake-katago --locked`，从 Cargo JSON 消息取得可执行文件路径，尊重目标目录及平台后缀。同一进程用互斥锁串行初始化，其他调用者等待后直接复用成功缓存的路径，避免并发构建删除并重新链接可执行文件的竞争；构建失败不缓存，后续调用可重试。仅供运行时测试调用，不用于构建脚本或产品代码。
- 会话测试的非目标运行超时大于外层 5 秒安全期限，避免慢机器误走故障路径。看门狗测试先观察一次性挂起标记，再推进 Tokio 虚拟时间；推进后立即恢复实时时钟，正常重放使用宽松超时。启动队列由外部文件释放，FIFO 和优先级通过长搜索占用单个搜索槽、逐次取消来验证，不依赖消费者被调度的先后顺序。

## 8. 与 Java 版的兼容

- 读写 SGF 的 `LZ` 分析属性（格式以 Java 版 `SGFParser`、`SgfAnalysisPayloadWriter` 为准，P2）。
- 编码：BOM、`CA`、UTF-8、GB18030 / GBK（参考 Java 版 `EncodingDetector`）。
- GIB（弈城）导入（P2）。
- 首次启动导入 Java 版的配置和引擎设置（P2）。
- 应用标识 `io.github.wimi321.lizzieyzy.next2`，配置目录独立，可与 Java 版同时安装。

### P1 预览打包

- `.github/workflows/preview.yml` 通过 workflow_dispatch 手动触发，沿用 CI 的 Rust、Node、pnpm 和 Tauri 系统依赖安装步骤，使用冻结的依赖锁文件。Windows 生成 NSIS、macOS 生成 DMG、Linux 生成 AppImage 和 deb；artifact 名含平台、架构与运行 ID。P1 显式使用 `--no-sign`，只携带默认分析配置，不捆绑 KataGo 或模型。桌面 crate 的默认 `dev-tools` 特性保留无额外参数的 export-bindings 命令；预览构建使用 `--no-default-features` 排除该开发工具二进制，Tauri 自身照常启用 `custom-protocol`。

## 9. 暂不做

GTP 后端、人机对局、陪练、远程算力、弈客、HumanSL 棋力评估、AI 解说、死活题、网页棋盘，见 ROADMAP 的 P3。readboard 本阶段仅实现 4.10 的 Windows 同步后端。
