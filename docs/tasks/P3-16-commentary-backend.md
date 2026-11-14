---
id: P3-16
title: AI 解说的后端：OpenAI 兼容服务、KataGo 依据、核对、写入棋谱评论
depends_on: [P3-14]
paths:
  - Cargo.toml
  - crates/commentary/**
  - crates/app-core/**
  - scripts/architecture.json
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
  - docs/ARCHITECTURE.md
---

## 目标

P3 第 6 批：AI 解说（Java 版 #390）。用大语言模型把 KataGo 已有的分析讲成人能读懂的话；模型只负责解释，判断以 KataGo 为准。Java 参考在 `../lizzieyzy-next` 的 `teacher/` 目录，只学行为：`TeacherDialog`、`TeacherEvidence`、`TeacherPromptBuilder`、`TeacherBoardContext`、`TeacherVerifier`、`GroundedCommentaryClient`、`TeacherLlmClient`、`TeacherSettings`、`TeacherCommentCodec`。

**和 Java 不同的地方（维护者决定）**

- 只做「API key」方式：任何 OpenAI 兼容的服务（OpenAI、DeepSeek、通义千问、Kimi、本机的 Ollama 等）。Java 的「ChatGPT 账号登录」依赖 OpenAI 的合作伙伴通道，这一批不做。
- 以下 Java 问题不照搬：
  - 回答其实不是边生成边显示；
  - 追问的回答会覆盖棋谱里原来的解说；
  - 「范围」解说存在当前节点上；
  - 提示词没写胜率是哪一方的；
  - 核对坐标时，棋盘上任何一颗子或气都算合法。

## 要做

1. **`crates/commentary`**：新的纯同步 crate，依赖只有 `go-core`。在 `scripts/architecture.json` 里登记（加进 `workspace_crates` 和 `pure_crates`，给 `app-core` 加上依赖），不改别的规则。负责：
   - **依据**：输入是 app-core 整理好的局面和分析数据，输出依据文本。每个局面写：
     - 手数、轮到哪方、根节点计算量；
     - 实际下的下一手；
     - 前 3 个候选点（坐标、计算量、胜率、目差）。胜率和目差一律换成**轮到的一方**的视角，并在依据里写明；
     - 实际下的那手不在前 3 时也列出；不在候选里时写明「不在候选中，损失未知」；
     - 实际那手相对首选的胜率损失（百分点）和目差损失（目）；
     - 棋盘大小，每一块棋的子和气（用 go-core 计算）；
     - 本地重放：实际后续最多 5 手、每个候选的变化最多 12 手。按规则重放，标出提子；不合法时停止并写明；剩 1–2 口气的块单独标出。
   - **关键局面的挑选**（范围和整盘）：照 Java，最多 5 个、相隔至少 3 手、按手数顺序排列。排序依次按实际那手的目差损失、胜率损失 ÷10、计算量。返回已分析、选中、略去的局面数。
   - **提示词**：system 和 user 两条消息，追问时再加上一条 assistant（上一次的回答，最多 12 000 字符）和一条 user（追问）。
     - 界面是中文时用中文提示词，照 Java 的要点：解释 KataGo 的判断，不替代它；只用给出的依据，不编造坐标、变化、胜率、目差、意图和结果；不指控作弊，不评段位，不自称老师或教练；目差 ≤0.5 视为相当，0.5–1.5 略好，>1.5 值得解释，胜率是次要的；繁体界面用繁体中文回答。
     - 其他语言用英文提示词，并要求用界面语言（日语、韩语、泰语、英语）回答。
     - 教学要求照 Java：每个局面一个具体的决定，联系到具体的棋子；先讲棋盘上的道理，再讲胜率；最多一个数字；不罗列候选统计；2–4 个短段落。
     - 用户的水平（18 级 ~ 9 段，默认 5 级）、风格（平衡 / 严谨 / 耐心 / 严格 / 幽默）、术语（少 / 中 / 多）、详略（简略 / 标准 / 详细）、变化（少 / 适中 / 多）写进提示词。
     - Java 的内置知识库片段不做。
     - 提示词总长上限 250 000 字符。
   - **核对**：
     - 回答里每一个坐标都必须出现在依据的着手里，包括候选点、变化、实际着手、后续着手和被点名的棋块的子；
     - 只认大写的 GTP 坐标；
     - 百分数不超过 100。
     - 返回问题列表。
   - **棋谱评论块**：编码和解码照 Java 的格式，`[LizzieYzy AI Commentary BEGIN]` … `[LizzieYzy AI Commentary END]`，含 `generatedAt`、`model`，正文最多 100 000 字符，转义正文里的标记。替换节点评论里旧的 AI 块，保留用户自己写的部分。追问的回答作为同一块里的「追问」小节追加，不覆盖原来的解说。
2. **OpenAI 兼容客户端**（app-core，用现有的 reqwest）：
   - 服务地址：必须是 https；只有 localhost、127.0.0.1、::1 可以用 http；拒绝带用户名密码、query 和 fragment 的地址；去掉末尾的 `/`；路径为空时补 `/v1`。
   - `POST {base}/chat/completions`，`stream: true`，按 SSE 读取 `choices[0].delta.content`，必须以 `finish_reason` 为 `stop` 或 `[DONE]` 结束，否则算中断。只在 404 / 405 时改用 `POST {base}/responses` 再试一次（读 `response.output_text.delta`，必须收到 `response.completed`）。
   - `GET {base}/models` 列出模型。
   - Bearer 认证；User-Agent `LizzieYzy-Next-AI-Commentary`；不跟随跳转；连接超时 15 秒；整个请求最长 5 分钟。
   - 错误类型化：未配置、没有分析、HTTP 状态（正文最多 240 字符，并去掉 key 和 Bearer 值）、429（带 `Retry-After`）、网络、超时、回答中断。日志里不能出现 key。
3. **API key 的保存**：
   - 新增依赖 `keyring`（系统钥匙串：macOS Keychain、Windows 凭据管理器、Linux Secret Service），写一行理由。用 trait 包一层，测试用内存实现，测试绝不碰真实钥匙串。
   - 不勾选「记住」时，key 只放在内存里；勾选时存进系统钥匙串，账户名按服务地址区分。换服务地址时删掉旧的 key。系统钥匙串不可用时只放内存，并告诉界面。
   - key 永远不写进 settings.json 和日志。
   - CI 三个平台都要能构建。Linux 上如果 Secret Service 需要的系统库导致 CI 构建失败，就用 `keyring` 的 vendored 选项，或在 Linux 上退回只放内存，写进汇报。不改 `.github/workflows/`。
4. **设置** `Settings.commentary`：服务地址、模型、是否记住 key、水平、风格、术语、详略、变化、「将解说加入棋谱评论」（默认开）。读写和校验照 P2-06 的约定，非法值返回类型化错误。
5. **解说会话**（app-core，同一时刻最多一个请求）：
   - `start_commentary(CommentaryRequest)`，三种模式：
     - 下一手：当前节点，对比实际的下一手；
     - 范围：主线上第 from 到第 to 手，每一手用它前面那个局面的依据；
     - 整盘：整条主线。
     - 加上可选的追问：沿用上一次的依据和回答。
   - 只用已经有的分析结果，包括实时分析、整盘分析、批量分析和从棋谱读入的分析，不向引擎请求新的分析。没有任何候选点时返回「还没有分析」。
   - **事件 `CommentaryUpdated`**：请求 id、阶段（准备 / 生成中 / 核对中 / 完成 / 失败 / 已停止）、新增的文字、选中的局面和依据全文（界面的「分析依据」页显示的就是发出去的内容）、核对问题、错误。
   - **流程**：
     1. 边生成边发出文字；
     2. 生成完后核对；
     3. 不通过就发一次更正请求（把草稿作为 assistant，再加一条 user：列出最多 8 个问题，要求只改正这些地方），用更正后的文字替换；
     4. 仍不通过就照常显示，但带上问题列表，并且不写入棋谱。
   - **写入棋谱**：开关打开、核对通过时写入。下一手 → 当前节点；范围 → 范围的第一手；整盘 → 根节点。写入是一次可撤销的编辑。
   - **停止**：`stop_commentary()`，以及换棋谱、关闭程序时取消请求。
   - `commentary_models()` 列出模型；`set_commentary_key(key, remember)`、`clear_commentary_key()`；`commentary_key_status()` 返回是否已有 key、是否存在系统钥匙串里、钥匙串是否可用。
6. **测试**：
   - 用本地假 HTTP 服务模拟 OpenAI 兼容接口，包括 SSE 分块、`[DONE]`、`length` 中断、404 时改走 `/responses`、429、错误正文里的 key 被去掉。
   - 绝不访问真实服务。联网验收写成 `#[ignore]` 的测试，地址、模型和 key 从环境变量读。
7. 新字段和新命令照约定：生成的 TS 里可选，mock 补默认值，mock 里是脚本化的 `command`。

## 不做

- 界面：P3-20。
- ChatGPT 账号登录、Java 的内置知识库、语音朗读。

## 验收标准

1. `commentary` crate：
   - 依据里视角写明并换成轮到的一方，白棋时数值正确地翻转；
   - 不在候选中时写「损失未知」；
   - 块和气、重放（提子、不合法时停止、标出 1–2 口气的块）；
   - 关键局面的挑选（上限、间隔、排序、顺序）；
   - 中文和英文提示词、追问消息、长度上限；
   - 核对的正反两面；
   - 评论块的编码、解码、替换、追问追加、标记转义。
   - 以上各有测试。
2. 客户端（用假 HTTP 服务）：地址校验、流式读取、中断、404 时退回 `/responses`、429、错误正文里 key 被去掉、`/models`。
3. key：不记住时只在内存，记住时进（假的）钥匙串，换地址删旧 key，钥匙串不可用时只放内存；settings.json 里永远没有 key（写测试读文件确认）。
4. 会话：三种模式选对局面；追问沿用依据；没有分析时报错；核对不通过时更正一次，仍不通过时不写入；写入的节点对三种模式都对，且可以撤销；停止和换棋谱时取消请求。
5. `scripts/verify.sh --e2e` 通过；新依赖每个写一行理由；`docs/ARCHITECTURE.md` 更新，包括第 2、3 节的 crate 和依赖方向，以及发给 AI 服务的内容（足以还原局面，不含对局者名字、棋谱信息和用户评论）。
