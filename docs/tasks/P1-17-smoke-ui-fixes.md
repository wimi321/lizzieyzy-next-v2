---
id: P1-17
title: 冒烟发现的界面问题
depends_on: [P1-10, P1-14]
paths:
  - apps/desktop/src/graph/**
  - apps/desktop/src/settings/**
  - apps/desktop/src/locales/*.json
  - apps/desktop/src/App.tsx
  - apps/desktop/src/App.test.tsx
  - apps/desktop/src/styles/**
  - apps/desktop/src-tauri/tauri.conf.json
  - apps/desktop/e2e/**
---

## 目标

修掉 macOS 手工冒烟时看到的三个界面问题。界面由 Claude 做。

## 要做

- 胜率图：只分析过一手、前后都没有分析的点，现在看不见，因为单点画不出线和面积。把这种点画成小圆点，胜率和目差都要。
- 引擎设置面板：已经有生效的引擎设置时再打开，说明文字和按钮改成"修改当前设置"：
  - 按钮叫"应用"；
  - 选择和当前设置一样、引擎也正常时，按钮不可用；
  - 引擎出错或未启动时，同样的设置也能再点一次。
  - 首次设置的界面不变。
- 窗口标题和顶栏去掉"(dev)"。

## 验收标准

1. Vitest：
   - 单点画成圆点；
   - 已有设置时显示"应用"，没改动且引擎正常时不可用，改动后可用；
   - 首次设置仍显示"启动"。
2. Playwright（mock 后端）通过。截图基线按流程在 Linux 上重新生成。
3. `scripts/verify.sh --e2e` 通过。
