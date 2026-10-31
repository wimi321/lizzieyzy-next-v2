---
id: P1-28
title: 引擎开箱即用：自动选好引擎，默认 B11 模型
depends_on: [P1-27]
paths:
  - crates/app-core/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/**
  - apps/desktop/e2e/**
  - resources/katago/**
  - docs/ARCHITECTURE.md
---

## 目标

维护者 2026-10-31：「打开后不需要弹引擎设置窗口吧，默认选好引擎就行了，引擎应该默认最新的 tf b11 模型吧」。

第一次打开时不弹设置窗口。能找到 KataGo 和模型，就自动配好并直接开始分析。

模型默认用 LizzieYzy Next 的默认模型 B11，即 `kata1-tf3-b11c768-s12002M-d6304M.bin.gz`。Java 版安装包里它叫 `weights/default.bin.gz`。

只有在什么都找不到，或者找到的组合都起不来时，才打开设置窗口，并说明原因。

分工：后端和 `src/api/` 由 Codex 做。`App.tsx`、`settings/`、`locales/` 里的界面接线由 Claude 在同一分支上做，Codex 不要改这几处。

## 要做

1. **模型目录。**
   - 把 Java 仓库的 `src/main/resources/katago-assets.json` 原样复制为 `resources/katago/katago-assets.json`。
   - 在 ARCHITECTURE.md 里记下它的来源（Java 提交 993e288）。
   - 只用其中的 `defaultModelId` 和 `models`：文件名、`sizeBytes`、`sha256`、`tier`、`minimumKataGoVersion`。
2. **扩大探测范围。** 新增两类位置：
   - 新版自己的资源目录：`<resource_dir>/engines/katago/<平台>/katago[.exe]` 和 `<resource_dir>/weights/*.bin.gz`。布局照 Java 版安装包。P1-34 会往里放文件，这张卡只负责找到它们。
   - 已安装的 LizzieYzy Next（Java 版）自带的引擎和模型：
     - macOS：程序在 `/Applications/LizzieYzy Next.app/Contents/app/engines/katago/macos-<arm64|amd64>/katago`，模型在 `.../Contents/app/weights/*.bin.gz`。`~/Applications` 下也要找。
     - Windows 和 Linux：默认安装位置照 Java 仓库的打包脚本确定。
     - 只找固定位置，不做全盘搜索。

   探测仍然只搜 `SearchRoots` 里显式给出的位置，测试里注入假目录。
3. **模型排序。**
   - 只有文件大小等于目录里某个模型的 `sizeBytes`，才计算它的 SHA-256。
   - 命中目录的模型排在最前面：目录默认模型（B11）第一，其余按 `tier` 排，顺序是 strongest、balanced、lightweight。
   - 其余文件沿用现在的规则。另外修好 `s12002M`、`d6304M` 这类带 M/G 后缀的计数，它们现在会被解析成 0。
   - 识别出的目录模型，在引擎配置里记下它的规范文件名，供界面显示引擎名。这样 `default.bin.gz` 也显示为 kata1-tf3-b11c768。
4. **自动配置。** app-core 新增一个命令，名字自定，例如 `auto_configure_engine`。
   - 已经保存了引擎配置时，什么都不做，原样返回现有配置。
   - 否则先探测，再按模型排序逐个试组合：
     - 每个模型先配同一安装里的程序，再配其他程序。
     - 试启动用现有逻辑（`engine_trial.rs`）。第一个成功的组合保存为引擎配置，并开始分析。
   - 试启动前先取程序版本。版本低于模型的 `minimumKataGoVersion` 时，跳过这对组合。
   - 试启动的次数和总时长都要设上限，具体取值写进汇报。
   - 失败时返回类型化错误，分三种：没找到程序、没找到模型、全部试启动失败。最后一种要附上每次失败的原因。
   - 同时补上 Tauri 命令、ts-rs 绑定、`api/backend.ts` 和 mock-backend。
5. **界面接线（Claude）。**
   - 首次启动时调用自动配置。失败才打开设置窗口，并显示原因。
   - 引擎名用第 3 步记下的规范文件名。

## 不做

- 下载模型或引擎（由 P1-34 放进安装包）。
- 测速调参。
- 重新设计设置窗口。

## 验收标准

1. 单元测试：
   - B11 排在 b28、b18 等其他 kata1 模型前面，靠 SHA-256 识别；改名为 `default.bin.gz` 也能认出；
   - 大小不符的文件不计算哈希；
   - `s12002M` 解析为 12002000000；
   - 用假目录时，能找到 Java 版安装目录和新版资源目录；
   - 版本不够的程序不与 B11 配对。
2. 集成测试（fake-katago）：
   - 没有配置时，自动配置成功、保存配置、开始分析；
   - 第一个组合试启动失败时，换下一个；
   - 全部失败时返回原因，不保存配置；
   - 已有配置时不做任何改动。
3. `scripts/verify.sh` 通过，绑定一致性测试通过。
