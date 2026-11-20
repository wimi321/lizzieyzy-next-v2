---
id: P3-41
title: 工程线：引擎输出的健壮性——非 UTF-8 字节、超长行、不停读 stderr
depends_on: [P3-40]
paths:
  - crates/engine-session/src/**
  - crates/engine-session/tests/**
  - crates/engine-session/Cargo.toml
  - tools/fake-katago/**
  - docs/ARCHITECTURE.md
---

## 目标

KataGo 分析会话和 GTP 会话都通过 `crates/engine-session/src/transport.rs` 读子进程的输出：`BufReader::lines()`，按行读成 `String`。2026-11-19 审查时发现三个问题：

1. **非 UTF-8 字节让引擎启动失败。** `next_line()` 遇到非 UTF-8 字节返回错误，transport 随即停止读这个流，并发出 `Output::Fault`；就绪之前收到的 Fault 是致命的（`actor.rs` 里 `fatal: !self.ready`）。KataGo 启动时会把配置和模型的路径写进 stderr。中文 Windows 上，路径里有中文时（比如用户名是中文），这些字节可能是系统代码页（GBK）而不是 UTF-8。Java 版按系统默认编码读，不会出这个问题。
2. **出错后不再读 stderr。** stderr 读出错以后 `stderr_open = false`，之后没人读这个管道。子进程继续往 stderr 写，写满管道缓冲区（几十 KB）就会卡住，看上去就是引擎卡死。
3. **单行长度没有上限。** 引擎输出一行不带换行的大量数据时，内存会一直涨。stderr 虽然有 `bounded_line`，但截断发生在整行读完之后。

readboard（`app-core/src/board_sync/process.rs`）和引擎版本探测（`auto_setup/version.rs`）已经按字节读、限制行长并一直读完 stderr，不在这张卡里。

## 要做

1. transport 改成按字节读 stdout 和 stderr，每行有上限，读的时候就限制，不先读完整行再截。
   - stdout 上限 8 MiB。正常的 KataGo 回复包括 ownership 和 policy 也远小于这个值。
   - stderr 每行只保留前 4096 字节，和现在的 `bounded_line` 一样。超出的部分边读边丢，不缓存。
   - 进程活着的时候，两个流都一直读到 EOF。读到坏字节、超长行都不能让 transport 停止读。只有真正的 I/O 错误才可以停，停的时候照现在的样子报 Fault。
2. stdout 的坏行不再是 transport 故障，按现有的「格式错误的行」处理：
   - 不是 UTF-8 的行，或者超长被丢掉的行，算一行格式错误的输出，计入 `bad_lines`。连续太多照现有规则处理（非致命 Fault）。
   - 实现方式你定：可以加一个 `Output` 变体，也可以把有损解码后的文本交给解析。远程会话（`self.remote`）的日志规则不变：不把原文写进日志。
   - GTP 会话（`gtp/actor.rs`）收到这样的行时，照它现有的解析错误规则处理。transport 不能因为它停止读，也不能把它当成 I/O 故障。
3. stderr 的解码：
   - 合法 UTF-8 照原样。
   - 不是 UTF-8 时，Windows 上按系统 ANSI 代码页解码（`MultiByteToWideChar(CP_ACP, …)`，同 Java 的默认编码），失败再用有损 UTF-8；其他平台用有损 UTF-8（`String::from_utf8_lossy`）。
   - 解码后的行照常用于就绪判断（`startup.observe`）、stderr 尾部和日志。
   - Windows 上需要 `windows-sys` 时，作为 engine-session 仅 Windows 的依赖加入（它已经在 `Cargo.lock` 里），汇报里写理由。
4. fake-katago 加开关，用来重现这些情况（名字你定，`--help` 里写明）：
   - 启动时往 stderr 写一行带 GBK 编码中文路径的日志，然后照常就绪；
   - 在第 N 个查询的回复前，往 stdout 写一行指定长度、不带换行的数据（再补上换行）；
   - 就绪后往 stderr 写一个非 UTF-8 字节，再连续写指定字节数（比如 1 MiB）的日志，同时照常回复查询。
5. 测试（用 fake-katago，不用真的 KataGo）：
   - 启动日志带 GBK 路径：会话能就绪；stderr 尾部里能看到这一行（macOS 和 Linux 上是有损解码后的样子；Windows 上的测试断言按代码页解码，CI 的 Windows runner 代码页是 1252 而不是 936，所以 Windows 断言用 `MultiByteToWideChar` 对同样的字节算出期望值，不写死中文）。
   - stdout 一行 9 MiB：这一行算一行格式错误，下一条回复照常送达，会话不重启。
   - stderr 先坏字节再 1 MiB 日志：查询照常得到回复，不卡住（有安全超时，≤ 5 s）。改之前这个测试应该卡住或失败。
   - 一行 stdout 刚好在上限以内：照常解析。
   - 这些测试在改之前失败、改之后通过，汇报里写清楚怎么确认的。
6. ARCHITECTURE 里 engine-session transport 的说明补上：按字节读，stdout 每行 8 MiB 上限，stderr 每行保留 4096 字节，坏行算格式错误，stderr 的解码规则，以及两个流始终读到 EOF。

## 不做

- 远程 transport（WebSocket、Socket.IO、SSH）：它们有自己的上限。
- readboard 和引擎版本探测。
- 改 KataGo 的启动参数或配置。

## 验收标准

- 上面的测试都在，改之前失败（或卡住到安全超时）、改之后通过。
- `cargo test -p engine-session` 和 `cargo test -p fake-katago` 通过。
- `scripts/verify.sh` 通过；`check_budgets.py` 通过；三个平台的 clippy 都通过（Windows 部分用 `cargo clippy --target x86_64-pc-windows-msvc` 检查不了时，在汇报里说明，留给 CI）。
