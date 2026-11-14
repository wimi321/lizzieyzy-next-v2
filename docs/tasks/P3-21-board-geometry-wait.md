---
id: P3-21
title: 工程线：棋盘点击测试在棋盘量好尺寸之前就点下去
depends_on: [P3-14]
paths:
  - apps/desktop/e2e/moves.spec.ts
  - apps/desktop/e2e/modes.spec.ts
---

## 目标

`e2e/moves.spec.ts` 的「right-clicking an empty point inserts a move; on a candidate it adds the line as a branch」偶尔失败：P3-13 时一次，P3-15 验收时一次（本机，单独连跑 10 次里也失败过一次）。现象都一样：开局后第一次右键，5 秒内菜单一直没有出现（`board-menu` 里 0 个菜单项）。

原因：

- 这个文件的 `start()` 只等 `data-ready`。`data-ready` 只说明棋子图片和字体已经加载好，不说明棋盘已经量好尺寸。
- 棋盘尺寸来自 `useElementSize` 里 ResizeObserver 的第一次回调。在那之前，`data-cell`、`data-origin-x`、`data-origin-y` 都是 `0.000`。
- 机器负载高时，`pixelOf()` 正好在这段时间读几何，算出的每个点都是 (0, 0)。右键于是落在棋盘元素的左上角，不在任何交叉点上，`Board` 不打开菜单。
- `board.spec.ts` 和 `display.spec.ts` 的 `start()` 已经等 `data-cell` 不为 `0.000`。`modes.spec.ts` 的写法和 `moves.spec.ts` 一样，也有这个问题。

## 要做

1. `moves.spec.ts` 和 `modes.spec.ts` 的 `start()` 在返回之前等 `data-cell` 不为 `0.000`，写法照 `board.spec.ts`。

## 不做

- 不改产品代码，不提高 5 秒超时，不加重试。

## 验收标准

1. 证明原因：用 `page.addInitScript` 把 ResizeObserver 的回调推迟 400 ms（临时改动，不提交），修改前这两个文件里读几何的测试失败，修改后通过。
2. `pnpm exec playwright test e2e/moves.spec.ts e2e/modes.spec.ts --repeat-each=30 --workers=10` 全部通过。
3. 本机整套 Playwright 通过；PR 上 CI 的 `scripts/verify.sh` 三个平台都通过（只改了两个 e2e 文件，Rust 部分交给 CI）。
