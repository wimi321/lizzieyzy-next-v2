---
id: P3-18
title: 智子云算力的后端：账号、Socket.IO 连接、GTP 到分析协议的桥接
depends_on: [P3-17]
paths:
  - crates/gtp/**
  - crates/engine-session/**
  - crates/app-core/**
  - tools/fake-gtp/**
  - fixtures/zhizi/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
  - docs/ARCHITECTURE.md
---

## 目标

P3 第 6 批：远程算力的第二部分，智子云算力。Java 参考在 `../lizzieyzy-next`，只学行为：

- `gui/RemoteComputeDialog.java`（「智子云算力」页签）、`gui/ZhiziAccountDetailsDialog.java`；
- `analysis/remote/ZhiziApiClient.java`、`analysis/remote/ZhiziGtpTransport.java`、`analysis/remote/ZhiziAccountService.java`、`analysis/remote/RemoteComputeConfig.java`、`analysis/remote/PlatformCredentialStore.java`。

智子的服务器上跑的是 KataGo 的 GTP 模式，通过 Socket.IO 收发 GTP 文本；新版的分析只走 analysis JSON 协议。**维护者决定：用一个桥接把 GTP 转成分析协议**。engine-session 照常发 analysis JSON 查询，桥接把每个查询翻成 GTP 命令和 `kata-analyze`，再把结果翻回 analysis JSON。这样协调器、缓存、整盘分析、批量分析、人机对局都能用智子，代价是整盘和批量分析要逐手重放，比本地慢。

**和 Java 不同的地方**

- 充值和开通 VIP 不在软件里收款，而是打开智子官网。
- 不照搬 Java 获取模型目录的非公开链路：临时账号、OSS 元数据、SSH 登录、`query-server`。
- 不保存密码，只保存登录 token。
- 「致命错误」按 HTTP 状态和错误码判断，不靠文字里的子串。
- 服务端给的连接地址必须是 https / wss。

## 要做

1. **`gtp` crate：解析 `kata-analyze` 的输出**（纯同步）。解析 `info move … visits … winrate … scoreLead … prior … order … pv …`、`rootInfo …` 和 `ownership …`；坏行返回类型化错误，不 panic。用 golden fixture 和 proptest 测试。
2. **桥接**（engine-session，包在任意一条 GTP 行传输外面，对外提供和本地进程一样的三个通道，收发的是 analysis JSON）：
   - `query_version` → GTP 的 `name` / `version`，回一个版本确认。
   - **分析查询排队**，同一时刻只跑一个：先按 priority，再按提交顺序。每个 `analyzeTurns` 依次处理：
     1. `boardsize`（矩形棋盘用 `rectangular_boardsize`）、`clear_board`、`komi`，再用 `kata-set-rules` 设规则；
     2. 摆子：引擎列出 `set_position` 时用它；没有时，含摆子的局面返回该查询的类型化错误；
     3. 逐手 `play`；
     4. `kata-analyze <轮走方> <间隔> ownership true rootInfo true`。间隔取 `reportDuringSearchEvery`，下限 0.1 秒。
   - **输出**：
     - 搜索中的每次报告翻成 `isDuringSearch: true` 的分析结果；
     - 达到 `maxVisits`（看根节点计算量）或 `overrideSettings.maxTime` 时发 `stop`，最后一次报告作为 Final；
     - 胜率、目差和 ownership 一律换成黑方视角。智子服务端按哪一方报告，看 Java 的 `analysis/Leelaz.java` 怎么解释 `kata-analyze` 的数值，照它做，写进 `ARCHITECTURE.md`。
   - **取消**（terminate）：正在跑的发 `stop`，先给出带已有计算量的 Final 再回确认；还在排队的给出 `noResults` 再回确认。和 4.5 节记录的 KataGo 行为一致。
   - **不支持的附加数据**（policy、按选点 ownership）不输出这些字段，不报错。
3. **Socket.IO 传输**（engine-session）：
   - 连接参数：`path=/socket.io.v4`，query `zz-socketio-token=<token>`，只用 websocket，连接超时 30 秒。等到 `ready` 事件（最多 60 秒）才算连上。
   - 上行：`emit("stdin", <GTP 文本，含换行>)`。下行：`stdout` 和 `stderr` 事件，负载可以是字符串或二进制，都按 UTF-8 解码。
   - 关闭时先发 `stdin "quit\n"`，再断开。
   - **每次连接（包括重启后重连）都要重新取一个会话 token**，具体做法见第 4 条。由 app-core 提供一个异步的连接函数给 engine-session，engine-session 本身不调用智子的 HTTP 接口。
   - 连接地址不是 https / wss 时拒绝。
   - 断线和重连沿用 engine-session 的看门狗和重启退避。遇到余额不足、VIP 失效、401/403 这类错误时，直接进入 Failed，不重试。
4. **智子账号**（app-core，用现有的 reqwest；基址 `https://www.zhizigo.com`，JSON，超时 20 秒）：
   - **接口**：照 Java 的接口表（`ZhiziApiClient.java`）：
     - 密码登录、验证码登录、发送验证码、重置密码；
     - 取会话 token（`fetch-socketio-token`，带 Bearer 和连接参数）；
     - 账号信息、余额、使用记录、资金记录。
     - 错误码取响应 JSON 的 `key`，映射成类型化错误：账号格式、账号或密码错误、验证码错误、请求太频繁、密码太短、账号已存在、账号不存在、登录过期、无法分配算力、网络、响应不完整，未知的错误码保留原值。错误里带上 request-id，不带账号和 token。
   - **账号和登录**：
     - 账号是手机号或邮箱（含 `@` 时按邮箱）。账号去掉首尾空白，密码一律不 trim。
     - 验证码登录在账号不存在时会自动注册，类型里要标明，界面会提示。
     - 发验证码后的冷却：用 `Retry-After`（最多 600 秒），没有就 60 秒。
     - 重置密码要求至少 8 位，成功后直接登录。
   - **登录 token**：
     - 「记住登录」（默认关）开着时，存进 P3-16 的钥匙串封装；关着时只在内存。
     - 换账号时删掉旧账号的 token；退出登录时全部清空；收到 401 时作废 token。
     - 不保存密码。账号（手机号或邮箱）不写进日志。
   - **账号概览**：账号信息、余额和最近 5 条使用记录并发拉取，缓存 45 秒。使用明细和资金明细每页 20 条。
   - **充值和开通 VIP**：只返回智子官网的 https 地址，由界面用系统浏览器打开。
5. **智子引擎**：
   - 引擎列表里加一种「智子」引擎，记下套餐和权重：
     - 套餐 7 种：VIP 包月（默认）、按量 1x/3x/6x/12x/24x、按量 1x-CUDA；
     - 权重用 Java 内置的名单（`RemoteComputeConfig.java` 590–630 行附近），默认 `10b512t`。
     - 连接参数照 Java 拼成，例如 `--platform all --engine-type go --gpu-type vip-share --kata-name katago-TENSORRT --kata-weight 10b512t`。
   - 智子引擎的指纹包括「智子」、套餐和权重。
   - P3-17 列出的远程引擎限制同样适用。
   - 没有登录时，智子引擎启动失败并给出「请先登录智子」，不影响其他引擎。程序启动时如果主引擎是智子而又没有登录，照 Java 的做法本次临时改用本地引擎，不改保存的设置。
6. **`tools/fake-gtp`**：支持 `kata-analyze`（按脚本输出 info / rootInfo / ownership 行、按间隔重复、收到 `stop` 或下一条命令时停止）、`set_position`、`rectangular_boardsize`、`kata-set-rules`。
7. **测试**：
   - 桥接用 fake-gtp 走本地进程传输，覆盖：实时分析、`maxVisits` / `maxTime`、多个 `analyzeTurns`、排队和优先级、取消（运行中和排队中）、白棋时翻成黑方视角、含摆子的局面、不支持的附加数据。
   - 再接上 app-core 证明整盘分析能跑完。
   - Socket.IO 传输：在测试里起一个本地 Socket.IO 服务，后面接 fake-gtp；覆盖 `ready`、上下行、`quit`、重连时重新取 token、非 https/wss 地址被拒。
   - 账号：本地假 HTTP 服务，覆盖每个接口、错误码映射、冷却、token 的保存和作废、日志里没有账号和 token。
   - 测试绝不访问真实的智子服务、外网和真实钥匙串。联网验收写成 `#[ignore]`，账号和密码从环境变量读。
8. 新字段和新命令照约定：生成的 TS 里可选，mock 补默认值，mock 里是脚本化的 `command`。

## 不做

- 在软件里充值或开通 VIP（微信扫码）。
- Java 获取模型目录的非公开链路。
- 智子之外的 GTP 引擎走桥接：第三方 GTP 引擎照旧只用来下棋（P3-10）。
- 界面：P3-20。

## 验收标准

1. `kata-analyze` 解析有 golden 和 proptest。
2. 桥接：第 7 条列出的每种情况各有测试，整盘分析经过桥接能跑完。
3. Socket.IO 传输：第 7 条列出的每种情况各有测试。
4. 账号：每个接口、错误码映射、冷却、记住和不记住登录、换账号、退出、401 作废，各有测试。settings.json 和日志里没有 token 和密码（读文件确认）。
5. 智子引擎：连接参数的拼法、指纹、未登录时的错误、启动时临时改用本地引擎，各有测试。
6. `scripts/verify.sh --e2e` 通过；新依赖每个写一行理由（Socket.IO 客户端、测试用的 Socket.IO 服务端）；`docs/ARCHITECTURE.md` 写明桥接、黑方视角的换算和发给智子的内容。
