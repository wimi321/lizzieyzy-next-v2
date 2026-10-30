# 验收流程

分工：维护者定范围、决定合并；Codex 按任务卡实现；Claude 做规划和验收。

## 1. 交给 Codex（复制使用，把 <ID> 换成卡号）

> 你在 lizzieyzy-next-v2 仓库工作。先完整阅读 AGENTS.md、docs/ARCHITECTURE.md 和任务卡 docs/tasks/<ID>-*.md。从 main 新建分支 task/<ID>-<简短英文名>，只修改任务卡 paths 范围内的文件。完成后运行 scripts/verify.sh（必须以 "verify: ALL PASSED" 结束）和任务卡列出的额外命令，然后按 docs/tasks/README.md 的汇报格式在对话里汇报。任务卡没说清楚、或必须改范围外的文件时，停下来在汇报里提问，不要自行扩大范围。

验收没通过时：

> 按以下验收意见修复，修完重新运行 scripts/verify.sh，并按原格式汇报：<粘贴 Claude 的修复清单>

## 2. 请 Claude 验收

对 Claude 说「验收 <ID>」（分支名不是默认格式时附上分支名）。Claude 的步骤：

1. 切到任务分支，运行 `scripts/verify.sh`；界面类任务加 `--e2e`。
2. 运行 `python3 scripts/check_scope.py docs/tasks/<ID>-*.md main`。
3. 对照任务卡逐条核对验收标准：每条都要有对应的测试或命令结果。抽查关键测试，确认它测的是行为；必要时临时破坏实现，看测试会不会失败（不提交）。
4. 审 diff：是否符合 AGENTS.md 和 ARCHITECTURE.md（依赖方向、actor 规则、类型生成、文件职责、没有证据型代码）。
5. 有手工项的任务，按卡里的步骤实际操作。
6. 改到 `fixtures/sgf/` 的任务：在装了 sgfmill 的虚拟环境里运行 `tools/fixture-oracle/generate.py --check`，必须通过（这一步需要 sgfmill，所以不在 verify.sh 里）。
7. 测试里会启动引擎进程或依赖计时的 crate（engine-session、analysis、app-core 等）：把这个 crate 的测试连续跑 10 遍，再在 CPU 满载下（按核数启动 `yes > /dev/null`）连跑 10 遍，全部通过。CI 的 macOS 机器只有 3 核，本机单跑能过不算数。

Claude 自己实现的界面任务，同样要过门禁和范围检查，交付时附截图给维护者看。

结论分三档，后面附「修复清单」，可以直接粘贴给 Codex：

- **通过**：可以合并。
- **有条件通过**：可以合并，列出的小问题放进下一张卡。
- **不通过**：只要有一条就不通过——verify 没全绿、缺验收项、测试不测行为、越界改动没有理由、违反架构规则。

## 3. 阶段验收

### P0 退出

- [ ] main 上 CI 三平台 verify 全绿
- [ ] `docs/inventory/FEATURES.md` 每一行都有维护者确定的层级
- [ ] 语料、KataGo 录制、翻译 JSON 已合并，并且有测试在用

### P1 退出

- [ ] P1-01 至 P1-22 全部通过
- [ ] macOS（Homebrew KataGo）、Windows、Linux 的安装包各手工跑一遍下面的冒烟
- [ ] 维护者连续一周用它做日常复盘，发现的问题都已建 issue

P1 手工冒烟（每个平台）：

1. 安装并启动；首次设置里选中 KataGo、模型和配置，状态变成 Ready。
2. 打开 `fixtures/sgf/` 里的一盘 19 路棋谱，棋盘和 Java 版显示一致。
3. 实时分析出现候选点；连续按 → 20 次，候选点始终属于当前局面，不会闪回旧局面。
4. 胜率图随分析更新；点击图上任一点，跳到对应手数。
5. 结束引擎进程（任务管理器或 kill），应用显示重启并自动恢复分析。
6. 关闭应用再打开，设置还在。

P1 性能目标：引擎已预热时，导航后首个候选点 ≤ 1 秒出现；连续推送分析时，界面主线程没有超过 50ms 的长任务（P1-09 自动测量）。
