---
id: P1-41
title: 主窗口严格还原（二）：棋盘
depends_on: [P1-40]
paths:
  - apps/desktop/src/**
  - apps/desktop/e2e/**
---

## 目标

棋盘照 Java 版 `BoardRenderer` 的 japanese 风格、default 主题严格还原。参照物和统一规则见 P1-40。界面由 Claude 做。

## 要做

1. **棋盘和棋子：**
   - 贴图：`board.png`、`black0.png`、`white0.png`；
   - 阴影、网格线粗细与颜色、星位；
   - 四边坐标的字体和位置。
2. **手数：**
   - 照维护者的设置，只在最后一手上显示手数；
   - 最后一手标记照 Java 版画。
3. **候选点：**
   - 圆的配色规则：最佳点青色带蓝圈，其余按胜率差配色；
   - 圆里三行：胜率、计算量、目差，字体、字号照 Java 版；
   - 右上角橙色序号；
   - 棋谱实际下一手画虚线圈；
   - 鼠标悬停时的变化预览照 Java 版。
4. **Kata形势：** 照 Java 版画，包括颜色和大小。
5. **棋盘下方的黑白名字牌：** 照 `BoardNamePlate` 画。
6. **空棋盘提示：** 照 `EmptyBoardHint` 画。

## 验收标准

1. 三种状态各与 Java 版截图对照，看不出差别：
   - 空棋盘带候选点；
   - 中盘；
   - 形势。
2. Vitest、Playwright 通过，截图基线重新生成。
