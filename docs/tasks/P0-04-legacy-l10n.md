---
id: P0-04
title: Java 版翻译转换为 JSON 参考词库
depends_on: [P0-01]
paths:
  - apps/desktop/src/locales/legacy/**
  - apps/desktop/src/locales/legacy.test.ts
  - tools/convert-l10n/**
---

## 目标

把 Java 版的语言包转成 JSON，作为新界面的翻译参考词库。新界面的正式词条另放在 `locales/{zh-CN,en}.json`，用到旧词条时再迁过去。生产代码不 import legacy 目录。

## 要做

- `tools/convert-l10n/convert.py`（Python 3.9，只用标准库）：
  - 读取 `../lizzieyzy-next/src/main/resources/l10n/DisplayStrings*.properties`。
  - 正确处理 `\uXXXX`、续行、`=` 和 `:` 分隔符、注释。
  - 按 BCP 47 命名输出 `apps/desktop/src/locales/legacy/<lang>.json`：键保持原样并排序，值原样保留，包括 `{0}` 占位符。
  - 支持 `--check`：重新转换并和已提交的文件比对。
- Vitest 用例 `apps/desktop/src/locales/legacy.test.ts`：
  - 各语言的键集合与英文一致。源文件本来就有的差异，在测试里列为已知例外。
  - 每个值的占位符集合与英文相同。

## 验收标准

1. 每种语言的键数与源 .properties 一致（汇报里列表）。
2. `python3 tools/convert-l10n/convert.py --check` 通过。
3. Vitest 用例通过；故意改坏一个值的占位符时会失败（汇报里说明怎么验证的，改动不提交）。
