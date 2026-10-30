---
id: P1-05
title: engine-session：每个引擎一个 actor
depends_on: [P1-04]
paths:
  - crates/engine-session/**
  - tools/fake-katago/**
  - Cargo.toml
  - docs/ARCHITECTURE.md
---

## 目标

按 ARCHITECTURE.md 4.5 实现引擎会话。这是整个重构里最关键的一块，取代 Java 版 2.5 万行的 Leelaz。

## 要做

- 实现 `spawn`、`EngineHandle`（submit / terminate / status / shutdown）、`QueryEvent`、`EngineStatus`，语义见 4.5。
- 传输层把进程 I/O 变成三个 channel：发出的行、收到的行、退出事件。P1 只实现本地进程。
- 实现以下行为：
  - 就绪检测、重启退避、致命错误分类。
  - 重启后重发进行中的请求。
  - 背压、坏行计数。
  - terminate 之后吞掉迟到的结果。
  - shutdown 后不留僵尸进程。
- 所有超时都可配置，测试里用短值。用 `tracing` 打日志，记录请求 id 和耗时。
- 测试辅助 `fake_katago::binary_path()` 被同一进程的多个测试线程同时调用时只构建一次（验收时发现的竞争：cargo 会先删掉再重新链接可执行文件，并发的线程会找不到它）。

## 不做

- 局面焦点、缓存、节流（P1-06 做）；GTP；SSH 和 WebSocket。

## 验收标准（全部用 fake-katago）

1. submit 之后先收到至少一个 Partial，再收到 Final；状态从 Starting 变为 Ready(version)；id 唯一。
2. 搜索中 terminate：订阅者只收到一个 Terminated，之后再没有任何事件；引擎迟到的输出被吞掉。
3. `--crash-after-queries`：状态 Restarting → Ready；进行中的请求以新 id 重发并完成；订阅者先收到 Restarted，再收到结果。
4. `--fail-startup`：进入 Failed，并带 stderr 末尾的内容；不会无限重启。
5. `--hang-after-queries`：看门狗超时后触发重启。
6. `--garbage-every`：坏行低于阈值时不重启，超过阈值时重启。
7. 慢订阅者只拿到最新的 Partial，但一定拿到 Final；channel 容量固定。
8. `--threads 1` 下，高 priority 的请求先完成。
9. shutdown 之后进程已经退出（等到退出码），状态为 Stopped。
10. 测试里不用 sleep 做同步；预算检查通过（没有 Mutex、RwLock、atomic、线程）。
11. `cargo test -p engine-session` 连续跑 10 遍全部通过。
