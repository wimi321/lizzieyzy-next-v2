---
id: P3-15
title: 腾讯棋谱和打开在线链接的后端
depends_on: [P3-14]
paths:
  - crates/fox-kifu/**
  - crates/app-core/**
  - fixtures/tencent/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
  - docs/ARCHITECTURE.md
---

## 目标

P3 第 6 批：在线棋谱的第一部分。Java 参考在 `../lizzieyzy-next`，只学行为：

- `gui/TencentKifuDownload.java`、`analysis/GetTencentRequest.java`：同步 → 腾讯棋谱；
- `gui/OnlineDialog.java` 的 `checkUrl` 和类型 99：文件 → 打开在线链接。

腾讯棋谱和野狐用同一类不需要登录的公开接口，单局下载 `TXWQFetchChess` 也是 `fox-kifu` 已经在调用的接口，所以放进 `fox-kifu` 这个 crate，照 4.9 节野狐的做法：同样的 HTTP 超时、重试、大小上限、SGF 整理和错误分类。

弈客、野狐/腾讯直播的实时同步不在本卡。

## 要做

1. **先确认接口还能用。** 用一个公开的腾讯围棋账号，分别请求一次列表接口和单局接口。如果接口已经失效（HTTP 错误、返回非零 `result`、或者格式和下面不一致），就停下来，把请求和响应摘要写进汇报，不要继续实现。汇报里不要写真实昵称和 UID。
2. **腾讯棋谱客户端**（`fox-kifu` 里的新模块）：
   - 查询 `TencentQuery`：昵称，或者纯数字 UID（纯数字时同时作为 `srcuid` 传）。
   - 列表：`GET https://cgi.huanle.qq.com/cgi-bin/CommonMobileCGI/TXWQFetchChessList`，参数 `type=7`、`lastCode=<0 或上一批最后一局的 chessid>`、`username`、`srcuid`、`txwqsession=lizzieyzy-next`、`fetchnum=100`。
     - `result` 为 0、且 `ret` 不存在或为 0 才算成功；否则返回类型化错误，带上 `resultstr`。
     - 每局取：`chessid`；开始时间（`starttime`，没有就取 `gamestarttime`，单位秒）；黑白双方的 UID、名字（依次取 `blacknick`、`blackenname`、`blackname`，白方同理）、段位；手数 `movenum`；胜方 `winner`（1 黑、2 白）；`point`（分数 ×100；-1 中盘胜，-2 超时胜）；`rule` 为 1 时按数子，单位是「子」。
     - 段位换算照 Java（`TencentKifuDownload.java:742`）：n ≥ 100 为职业 `P(n-99)`；n-17 > 0 为 `(n-17)` 段；否则 `(|n-17|+1)` 级。和野狐一样，界面只拿到结构化的段位，文案由界面本地化。
     - 翻页：每批最多 100 局，不足 100 局表示没有下一页；游标是本批最后一局的 `chessid`；游标重复时报错。
   - 下载：`GET https://happyapp.huanle.qq.com/cgi-bin/CommonMobileCGI/TXWQFetchChess?chessid=…`，返回 `{result:0, chess:"<SGF>"}`。整理沿用野狐的规则（去 BOM、去掉属性值外的反斜杠等）。
3. **app-core**：
   - 新命令 `tencent_search(query)`、`tencent_next_page(query, cursor)`、`tencent_open_game(chessid)`、`tencent_clear_history()`，照 `fox_*` 的做法：网络请求不占用 app actor，下载成功后走 `open_bytes` 的打开流程，失败时保留当前棋谱。
   - `Settings.tencent_history`：成功取得第一页后记下查询（昵称或 UID），去重，最近的在前，最多 10 条，可以清空。
4. **打开在线链接** `open_url(url)`：
   - 只接受 `http` / `https`，其余返回类型化错误。去掉首尾空白和引号。
   - 能识别的链接：
     - 野狐分享链接：query 里有 `chessid`（例如 `share.foxwq.com/index.html?...&chessid=...`）时，用 `fox-kifu` 的 `download(chessid)` 下载；
     - 弈客的链接（域名是 `yikeweiqi.com`）：返回类型化错误「暂不支持弈客链接」，留给以后的卡；
     - 其他链接：`GET` 一次，正文当作棋谱，用现有的打开流程（编码由 sgf crate 判断）。解析不出棋谱时返回类型化错误，不打开。
   - 超时和大小上限和野狐一样（单个响应最多 4 MiB）；跳转最多 5 次，跳转到非 http/https 时报错。
   - 只拉一次，不做实时刷新。
5. **测试数据**：`fixtures/tencent/` 放真实录制，照 `fixtures/fox/` 的做法脱敏：列表裁到 5 局，昵称、UID 和 SGF 里的 PB/PW 一致替换，保留字段类型和对局结果。测试注释写明录制日期和接口。`open_url` 用本地假 HTTP 服务测试。联网验收写成 `#[ignore]` 的测试，账号从环境变量读，不写进仓库。
6. **IPC 和 mock**：Tauri 命令放进新的 `commands/tencent.rs`（`open_url` 可以放进现有合适的文件），错误照 `FoxFailure` 的做法分出网络、超时、HTTP、接口错误，只传类型和数据，文案由界面本地化。新类型在生成的 TS 里可选，mock 里是脚本化的 `command`。

## 不做

- 界面：P3-19。
- 弈客：等维护者决定后另开卡。
- 野狐/腾讯/欢乐围棋直播的实时同步（私有二进制 WebSocket 协议，大概率已失效），欢乐围棋分享链接，`weiqi.qq.com/openqipu`。

## 验收标准

1. 汇报里写明第 1 条的结果：两个接口都还能用。
2. 腾讯客户端：查询、分页（包括不足 100 局和游标重复）、段位换算的三种情况、结果的四种情况（点数、子数、中盘、超时）、下载和整理、非零 `result` 和网络错误的分类，都有测试，用录制的数据和本地假 HTTP 服务。
3. app-core：搜索、下一页、打开（失败时保留当前棋谱）、历史记录的去重和上限、清空，各有测试。
4. `open_url`：非 http/https 被拒、野狐分享链接走 `download`、弈客链接返回暂不支持、普通链接打开、不是棋谱时报错且不打开、超过大小上限、跳转到非 http 被拒，各有测试。
5. `scripts/verify.sh --e2e` 通过；新依赖每个写一行理由；`docs/ARCHITECTURE.md` 的 4.9 节补上腾讯棋谱和打开在线链接。
