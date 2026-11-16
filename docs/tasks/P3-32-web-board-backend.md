---
id: P3-32
title: 网页棋盘的后端：局域网旁观服务、访问令牌、推送、旁观者试下
depends_on: [P3-30]
paths:
  - Cargo.toml
  - crates/app-core/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
  - docs/ARCHITECTURE.md
---

## 目标

P3 第 9 批「网页棋盘」（Java 的 Web 旁观）的后端。局域网里的人用浏览器打开一个地址，看到主窗口的棋局和分析，还可以试下。界面在 P3-33：主窗口的菜单和状态，以及浏览器里的网页。

Java 参考只学行为（`../lizzieyzy-next`，路径相对 `src/main/java/featurecat/lizzie/`）：

- `gui/web/` 全部：
  - `WebBoardManager.java`：启停、端口、地址、试下；
  - `WebBoardDataCollector.java`：消息内容；
  - `WebBoardUpdateQueue.java`、`WebBoardClientUpdates.java`：节流和每个连接只留最新一条；
  - `WebBoardHttpServer.java`、`WebBoardServer.java`；
- 网页：`src/main/resources/web/`（`board.js` 里的消息处理）；
- 菜单：`gui/Menu.java` 4054–4122。

**和 Java 不同的地方**（决定见 ROADMAP 第 9 批）：

- 一个端口同时服务网页和 WebSocket（Java 用两个端口）。
- 每次开启生成随机访问令牌放进地址，网页和 WebSocket 都要令牌，WebSocket 还检查来源。Java 不认证，`Access-Control-Allow-Origin: *`，主机浏览器里打开的任何网页都能连上。
- 试下用新版现有的试下（`trial_copy()` 的临时文档）。结束后主棋谱不留痕迹；Java 把试下的手永久加进主棋谱。落子规则（包括劫）由 go-core 判断。
- 试下的归属由服务端分配，不信任网页自报的 ID。
- 可以关掉「允许旁观者试下」，默认开。
- 地址按局域网私有地址优先挑选，并列出全部候选。

## 要做

1. **服务**（app-core 新模块，例如 `web_board/`；用已有的 hyper 和 tokio-tungstenite，需要时打开它们的 server 特性，不加新的库）
   - 绑定 `0.0.0.0`，默认端口 9998，被占用时依次试 9999…10007，都不行返回类型化错误。
   - 同一个端口：
     - `GET /<令牌>/` 返回网页的入口文件；
     - `GET /<令牌>/assets/…` 返回网页的静态文件；
     - `GET /<令牌>/ws` 升级为 WebSocket。
     - 其他路径一律 404，不区分令牌错和文件不存在。
   - 令牌：每次开启用系统随机数生成 128 位，base64url。
   - WebSocket 的 `Origin` 必须等于本服务的 `http://<Host 头>`，否则拒绝。
   - 只接受 GET；请求头和 WebSocket 消息大小有上限（例如 8 KB、4 KB）；连接数上限 20，超过的以 1013 关闭。
   - 静态文件由外壳提供：app-core 定义一个只读接口（路径 → 内容和类型），Tauri 外壳从内嵌的前端资源里取 `web.html` 和 `assets/` 下的文件，测试用内存里的表。路径含 `..`、反斜杠或编码后的分隔符一律拒绝。
   - 开关状态不保存，启动时总是关（同 Java）；退出时关闭服务，先结束试下。
2. **地址**
   - 列出本机所有已启用的非回环 IPv4 地址。排序：私有网段（192.168/16、10/8、172.16/12）在前，169.254 链路本地和明显的虚拟网卡（名字带 Virtual、VMware、vEthernet、Docker、utun 一类）在后。
   - 主地址是排第一的。访问地址 = `http://<地址>:<端口>/<令牌>/`。
   - 排序写成纯函数，单元测试。
3. **推送**（消息类型由 Rust 定义并生成 TS，网页和主窗口共用）
   - `FullState`：棋盘尺寸、棋子、最后一手、手数、轮走方、候选点（坐标、胜率、访问数、目差、目差标准差、policy、LCB、顺序、变化）、胜率、目差、总访问数、ownership（有时）。全部按黑方视角，同新版其他地方。
   - `AnalysisUpdate`：同上，去掉棋子。
   - `WinrateHistory`：从根到当前节点每一手的胜率和目差，没有分析的为空。
   - `ProblemMoves`：问题手列表，用新版现有的问题手查询和默认阈值。
   - `TrialState`：见下。
   - `Hello`：给新连接的 `client_id`（服务端分配）、规则文字（贴目、规则名）、是否允许试下。
   - 节流：分析更新每个连接最多每 100 ms 一条；局面变化立即发，并取代还没发出的分析更新。每个连接只保留最新的一条待发消息（每类一条），发送缓冲未清空时不再塞。新连接立即收到当前的全部状态。
   - 推送的内容跟随主窗口当前显示的节点：主窗口切换文档或节点、分析更新、引擎换掉时都推。
4. **旁观者试下**
   - 消息：`EnterTrial`、`ExitTrial`、`TrialMove { x, y }`、`TrialBack`、`TrialForward`、`TrialToAnchor`、`TrialChild { index }`。
   - 同时只有一个试下。归属是服务端分配的 `client_id`；连接断开 30 秒内同一网页带着服务端给的重连凭据回来，可以继续，否则试下结束。
   - 以下情况拒绝，并回 `TrialDenied { reason }`，`reason` 为 `InUse`、`Disabled`、`HostBusy`：
     - 别人正在试下；
     - 「允许旁观者试下」关闭；
     - 主窗口正在对局、试下、摆子或测速。
   - 开始：主窗口进入新版现有的试下（临时文档，从当前节点开始），引擎跟着分析。
   - 落子由 go-core 判断合法性：越界、有子、自杀、劫都拒绝，回 `TrialRejected`，不断开连接。
   - 后退、前进、回到起点、选分支都在临时文档里做。
   - 结束：旁观者退出、主窗口「强制结束试下」、空闲 5 分钟（同 Java）、服务关闭，都恢复原棋谱和原来的节点，主棋谱没有任何改动。
   - `TrialState { active, owner, anchor_move, move_number, can_back, can_forward, branches: [{ x, y, label, index }] }` 每次变化广播。
   - 试下每秒最多 10 条消息，超过的丢弃。
5. **命令和事件**（只授权主窗口）
   - `start_web_board() -> WebBoardStatus`；
   - `stop_web_board()`；
   - `web_board_status()`；
   - `set_web_board_options { allow_trial }`：保存在设置里，默认 true；
   - `force_end_web_trial()`。
   - `WebBoardStatus { running, port, url, addresses: [url…], viewers, trial: Option<{ owner, since }> }`，变化时发事件 `WebBoardChanged`。
6. Windows 第一次绑定 `0.0.0.0` 会弹出防火墙提示，这是预期的。ARCHITECTURE 里写明，界面卡会提示。

## 不做

- 浏览器里的网页和主窗口的界面（P3-33）。
- 外网访问、内网穿透、HTTPS。
- 旁观者翻看棋谱、看评论（Java 也没有）。

## 验收标准

- 用真实的本地服务（`127.0.0.1`，端口 0 或测试端口）和 tokio-tungstenite 客户端测：
  - 令牌错、路径穿越、`Origin` 不对、非 GET、超大消息、超过连接数，各被拒绝；
  - 新连接收到 `Hello` 和全部状态；
  - 主窗口换节点、分析更新时收到对应消息，100 ms 节流和「只留最新」成立；
  - 试下的完整流程：进入、落子、劫被拒、后退前进、选分支、退出后主棋谱不变；
  - 第二个人被拒；
  - 断线重连在时限内继续、超时结束；
  - 强制结束；
  - 空闲结束（用 tokio 的暂停时间推进，暂停时不要有活着的真实 socket；需要真实 socket 的部分用很短的真实时限）；
  - 关闭服务时试下结束、连接关闭。
- 地址排序的纯函数测试；端口被占用时顺延。
- 新命令只授权给主窗口；生成的 TS 类型和 mock 同步。
- `scripts/verify.sh --e2e` 通过；`check_budgets.py` 通过。
