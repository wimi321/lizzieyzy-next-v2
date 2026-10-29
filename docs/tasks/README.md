# 任务卡

- 一张卡 = 一个分支 = 一次验收。格式：front matter（`id`、`title`、`depends_on`、`paths`）+ 目标 / 要做 / 不做 / 验收标准。
- `paths` 是允许修改的范围（glob，`**` 表示目录下全部）；`Cargo.lock` 和 `apps/desktop/pnpm-lock.yaml` 总是允许。
- 状态以 GitHub 的 PR / issue 为准，仓库里不写状态文件。
- 下一阶段的卡在本阶段验收后再写（见 ROADMAP）。
- 怎么交给 Codex、怎么验收，见 `docs/ACCEPTANCE.md`。

## 汇报格式（卡里没有另外规定时使用）

1. 改了什么（3–8 行）。
2. 验收标准逐条对应：测试名或命令，加结果。
3. `scripts/verify.sh` 输出的最后 15 行。
4. 新增的依赖，每个一句话理由。
5. 没做完或拿不准的地方，需要维护者决定的问题。
