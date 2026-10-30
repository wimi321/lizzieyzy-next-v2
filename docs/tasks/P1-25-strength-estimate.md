---
id: P1-25
title: 棋力评估：后端
depends_on: [P1-24]
paths:
  - crates/strength/**
  - crates/app-core/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
  - fixtures/strength/**
  - Cargo.toml
  - scripts/architecture.json
  - docs/ARCHITECTURE.md
---

## 目标

维护者日常复盘会看棋力评估：根据整盘分析的结果，估出黑白双方的段位，并给出吻合度、平均损失等统计。

功能清单里棋力评估原本在 P3，维护者要求提前。这次只做基于分析结果的评估；HumanSL 那套要额外下载模型，以后再说。界面由 Claude 做（P1-26）。

## 要做

- 行为参照 Java 版：
  - `src/main/java/featurecat/lizzie/analysis/PlayerStrengthEstimator.java`；
  - `src/main/java/featurecat/lizzie/analysis/XGBoostStrengthModel.java`；
  - `src/main/java/featurecat/lizzie/gui/StrengthStats.java`、`StrengthScale.java`、`PlayerStrengthRankFormatter.java`；
  - 以上各文件对应的测试。
- 新建 crate `crates/strength`，纯计算，不依赖网络和异步运行时：
  - 从每一手的分析结果（P1-23、P1-24 的数据）提取特征，和 Java 版一致；
  - 读取 XGBoost 模型 JSON，做推理，再经过残差校准，得出每一方的段位估计；
  - 段位文字的格式（段、级、职业）照 Java 版。
- 模型文件从 Java 仓库原样复制到 `crates/strength/models/`，记录来源路径和 SHA-256：
  - `src/main/resources/models/strength/xgboost20tun_booster.json`；
  - `src/main/resources/models/strength/xgboost20tun_residual_calibrator.json`。
- 统计项照 Java 版的评估窗口：
  - 吻合度（与引擎首选一致的比例，Java 版还有的其他吻合口径也一并给出）；
  - 平均胜率损失、平均目差损失；
  - 开局、中盘、官子分阶段的数据，阶段划分照 Java 版。
- 分析不完整时，照 Java 版的规则给出"数据不足"，或者带上可信度说明。
- 在 `scripts/architecture.json` 里登记新 crate。这张卡只允许改三处：
  - `workspace_crates` 加 `"strength": []`，方括号里只写它真正需要的纯 crate；
  - `pure_crates` 加 `"strength"`；
  - `app-core` 的依赖列表加 `"strength"`。
- app-core 提供查询命令，整盘分析更新时结果跟着更新。提供 Tauri 命令和 ts-rs 绑定，mock-backend 跟着更新。

## 验收标准

1. 对照测试：用 Java 测试里的样例数据，或用 Java 版对同一份分析数据算出的结果做成 `fixtures/strength/`。Rust 算出的特征、段位和各项统计与 Java 一致，浮点数误差在 1e-6 以内。
2. 单元测试：
   - 模型推理：单棵树、多棵树、缺失特征的默认分支；
   - 段位格式化；
   - 数据不足的情况。
3. 集成测试（fake-katago）：整盘分析完成后，能查询到双方的评估结果。
4. 模型文件的 SHA-256 与记录一致，有测试校验。
5. `scripts/verify.sh` 通过，绑定一致性测试通过。
