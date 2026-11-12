---
id: P2-08
title: 文件和缓存（一）后端：GIB 导入、SGF 里的 LZ 分析数据、从系统打开棋谱
depends_on: [P2-06]
paths:
  - crates/sgf/**
  - crates/document/**
  - crates/app-core/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
  - fixtures/**
  - docs/ARCHITECTURE.md
---

## 目标

P2 第 3 批「文件和缓存」的前一半。旧用户手里的棋谱要能直接打开：弈城的 GIB、Java 版存了分析结果的 SGF，以及在访达、资源管理器里双击或拖到程序图标上的棋谱。Java 参考在 `../lizzieyzy-next`，只学行为：`rules/GIBParser.java`、`rules/SGFParser.java` 里读写 `LZ` 的部分、`rules/SgfAnalysisPayloadWriter.java`，测试 `SGFAnalysisWrnRoundTripTest.java`。

## 要做

1. **GIB 导入。** 打开、选择文件、拖入都认 `.gib`：对局双方、段位、贴目、让子、结果、日期和全部着手照 `GIBParser`。编码照现有的 SGF 解码规则（GIB 常见 EUC-KR、GB18030）。读进来就是一份普通的棋谱，保存时存成 SGF，「保存」第一次要问存到哪里（不覆盖原来的 GIB）。选择文件对话框的过滤器加上 GIB。
2. **读 LZ 分析数据。** Java 存在节点上的分析结果（`LZ` 属性，含胜率、计算量、目差和候选点；以及 Java 写的其他相关属性，照 `SGFParser`）读进来以后：
   - 胜率图、信息面板和问题手直接用这些值，就像这一手已经分析过一样，不需要引擎；
   - 当前局面的候选点也能显示（来源标为已存数据，引擎开始分析后由新结果替换）；
   - 解析失败的 `LZ` 值不影响打开棋谱，只在警告里记一条。
3. **写 LZ 分析数据。** 保存时把每一手已有的分析结果写成 Java 能读回的 `LZ` 属性，格式照 `SgfAnalysisPayloadWriter`。新版存、Java 版打开，以及反过来，都要能读到同样的胜率、计算量、目差和候选点。
4. **从系统打开。**
   - 安装包登记 `.sgf`、`.gib` 的文件关联（macOS、Windows、Linux 各自的方式，写在 Tauri 配置里）；
   - macOS 上程序已经在运行时，访达双击、拖到 Dock 图标、`open -a` 送来的文件要打开（Tauri 的 `RunEvent::Opened`）；程序启动时随之送来的文件也要打开；
   - Windows、Linux 上命令行参数里的棋谱路径在启动时打开；程序已在运行时再次双击，交给已在运行的那个窗口打开，不开第二个程序；
   - 打开失败照现在拖入失败的处理，发 `OpenFailed`。
5. 新字段照约定：生成的 TS 里可选，`e2e/mock-backend.ts` 补默认值。

## 不做

- 分析结果缓存：P2-09。
- 界面：「文件 → 保存」时是否带分析结果的开关等，由界面卡接。

## 验收标准

1. GIB：`fixtures/` 下至少两份 GIB（一份 UTF-8、一份 EUC-KR 或 GB18030，可以照 `GIBParser` 的格式手写，注明来源），黄金测试覆盖双方信息、让子、结果和着手。
2. LZ：Java 写的带分析 SGF 读入后，胜率图的点、信息面板的值和候选点与文件里的一致；新版写出再读回一致（往返性质测试）；坏的 `LZ` 值只产生警告。
3. 从系统打开：Tauri 层有测试证明 `Opened` 事件和启动参数里的路径走同一条打开路径；文件关联写进配置。汇报里写明用了什么插件或办法让第二次启动交给已在运行的程序，新依赖照规矩写一行理由。
4. `docs/ARCHITECTURE.md` 相关章节更新。
