---
id: P3-17
title: 远程算力的后端：自建算力（WebSocket）和 SSH 引擎
depends_on: [P3-16]
paths:
  - crates/engine-session/**
  - crates/analysis/**
  - crates/app-core/**
  - tools/fake-katago/**
  - fixtures/remote/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
  - docs/ARCHITECTURE.md
---

## 目标

P3 第 6 批：远程算力的第一部分。让 KataGo 跑在别的机器上，比如云主机或局域网里有显卡的机器，本机照常复盘。Java 参考在 `../lizzieyzy-next`，只学行为：

- `gui/RemoteComputeDialog.java` 的「自建算力」页签；
- `analysis/remote/KataGoAnalysisWebSocketTransport.java`、`analysis/remote/RemoteComputeConfig.java`（链接规范化、切换到本地引擎）；
- `gui/RemoteEngineSettings.java`、`analysis/ssh/`（SSH 引擎、known_hosts）。

新版的 KataGo 只走分析引擎的 JSON 协议。`ARCHITECTURE.md` 4.5 节已经留好接口：远程传输只要提供和本地进程一样的三个通道，即发出行、收到行和退出。自建算力本身就是 KataGo 的 analysis JSON，SSH 上远程执行的也是 `katago analysis …`，所以两者都直接接进现有的 engine-session。协调器、缓存、整盘分析和批量分析都不用改。

智子云算力是 P3-18。

## 要做

1. **WebSocket 传输** `TransportSpec::WebSocket { url }`：
   - 支持 `ws://` 和 `wss://`。`wss` 用 rustls 校验证书，不提供关闭校验的选项。连接超时 20 秒。
   - 三个通道：
     - 收到的每个 text 帧按行拆开，作为「收到行」；
     - 每个「发出行」作为一个 text 帧发出；
     - 连接关闭或出错作为「退出」，带上原因。
   - 握手、看门狗、重启、取消（terminate）沿用 engine-session 现有的逻辑，断线后按现有的重启退避重连。
2. **SSH 传输** `TransportSpec::Ssh { host, port, user, auth, command }`：
   - 用内置的 SSH 客户端，不依赖系统的 ssh 程序。
   - 认证：密码，或私钥文件（支持带口令的私钥）。
   - 远程执行 `command`，例如 `katago analysis -model … -config …`。stdout 作为「收到行」；stderr 进 engine-session 现有的 stderr 尾部；通道关闭作为「退出」。
   - 主机密钥照 Java 的做法首次信任，记在应用数据目录的 `ssh/known_hosts` 里。之后密钥变了就拒绝连接，而且要在发送任何凭据之前拒绝。错误里带上新旧指纹和文件路径。
   - 用户把 `ssh` 写成本地引擎的程序名，这种用法现在已经能用，不受影响。
3. **黑方视角**：本机的 KataGo 靠应用自带的配置报告黑方视角，远程的配置不归我们管。所以远程引擎的每个查询都要在 `overrideSettings` 里带上 `reportAnalysisWinratesAs: "BLACK"`。写测试证明，并写进 `ARCHITECTURE.md`。
4. **引擎列表**：
   - `EngineEntry` 可以是远程引擎，记下远程类型、地址、端口、用户、命令和私钥路径。
   - 秘密信息不进 settings.json，存进 P3-16 的系统钥匙串封装：自建算力的链接（链接里可能带访问令牌）、SSH 密码、私钥口令。钥匙串不可用时只放内存，只在本次运行有效。
   - 引擎指纹按远程身份计算，用来隔离缓存，包括：传输类型、主机、端口、路径或命令，以及 `query_version` 返回的版本。
   - 新命令：
     - 增加和修改远程引擎：可以和现有的引擎增删改合在一起，选最自然的做法；
     - `test_remote_engine(entry)`：连上、拿到版本后断开，返回版本或类型化错误；
     - `normalize_remote_link(text)`：照 Java 的规则，全角转半角，去掉两端的引号，从 `ws://` / `wss://` 截到第一个空白或引号为止；只接受带 host 的 ws/wss。Java 里 `完善://` 的特殊处理不要。
     - `read_remote_link_qr(path)`：从图片文件里识别二维码，再做规范化；不是图片或没有二维码时返回类型化错误。
   - 「切换到本地引擎」：选出第一个可用的本地引擎（程序、模型、配置文件都存在）设为主引擎；没有时返回类型化错误。
5. **远程引擎的限制**：主引擎是远程引擎时，一键测速、自动配置、线程推荐、AI 陪练加载人类模型都返回类型化错误「远程引擎不支持」，不去尝试。其余功能照常。
6. **测试**：
   - WebSocket：在测试里起一个本地 WebSocket 服务，后面接 `tools/fake-katago`。覆盖分析、取消、断线重连、一帧多行；证书错误可以只测 `wss` 连不上自签名的服务。
   - SSH：在测试里起一个本地 SSH 服务（同一个 SSH 库通常自带服务端），执行 fake-katago。覆盖密码、私钥、带口令的私钥、首次信任、主机密钥变化时在发送凭据之前拒绝、认证失败。
   - 黑方视角的 override。
   - 秘密信息不进 settings.json（读文件确认），也不进日志。
   - 链接规范化；二维码识别，用 `fixtures/remote/` 里生成的小测试图片。
   - 测试绝不访问外网，也绝不碰真实的系统钥匙串。
7. 新字段和新命令照约定：生成的 TS 里可选，mock 补默认值，mock 里是脚本化的 `command`。

## 不做

- 智子云算力：P3-18。
- 界面：P3-20。
- SSH 端口转发、跳板机、ssh-agent。

## 验收标准

1. 两种传输都能完成启动握手、实时分析、整盘分析、取消和断线后重连，各用 fake-katago 证明。
2. SSH 的密码、私钥、带口令的私钥、首次信任、主机密钥变化时拒绝（且没有发出凭据）、认证失败，各有测试。
3. 远程查询带上黑方视角的 override，本地查询的行为不变。
4. 秘密信息只在钥匙串或内存里，settings.json 和日志里都没有；引擎指纹随远程身份变化。
5. 链接规范化、二维码识别、切换到本地引擎、远程引擎的限制，各有测试。
6. `scripts/verify.sh --e2e` 通过；新依赖每个写一行理由（WebSocket、SSH、二维码各需要什么）；`docs/ARCHITECTURE.md` 4.5 节写明新的两种传输。
