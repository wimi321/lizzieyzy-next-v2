# Java 版功能清单

供两位维护者决定新版取舍；所有层级均为建议，审阅后由维护者将「建议层级」改成「层级」。本清单记录 Java 版的用户行为、入口和依赖，不要求迁移 Java 的线程、全局状态或界面结构。

## 阅读口径

- 参考仓库：只读 `../lizzieyzy-next`，提交 [`993e288f6b6dab71e0476a1b44ecf416cba71cf5`](https://github.com/wimi321/lizzieyzy-next/tree/993e288f6b6dab71e0476a1b44ecf416cba71cf5)。功能条目以该源码快照为准，下载统计为独立的在线时间快照。
- 功能名优先取 `src/main/resources/l10n/DisplayStrings_zh_CN.properties`；菜单路径省去快捷键和装饰字符。名称后的反引号 ID 是 `FunctionCatalog.entries()` 的稳定标识；`extra.*` 是本清单补充项，不冒充搜索目录 ID。
- Java 源码列中 `gui/`、`rules/`、`analysis/`、`update/` 等路径均相对参考仓库的 `src/main/java/featurecat/lizzie/`；`scripts/`、`src/main/resources/` 则相对参考仓库根目录。`Menu.java` 的叶子动作及 `FunctionSearchController.java` 的路由可配合定位实现。
- **MVP**：新版 [ROADMAP](../ROADMAP.md) P2「复盘 MVP / 2.0 预览版」必须具备的能力，包含 P1 主线；不是另建一个早于 P2 的发布阶段。**P2**：建议同阶段按余量补充的细化体验，不作为最小预览版阻塞项。**P3**：按需求补齐，含路线图明确后置的能力。**候选砍掉**：需要维护者决定是否删除、合并或改为可选组件，当前不会据此删除任何功能。
- 依赖列描述 Java 行为所需的引擎、网络、平台或本地资源；「本地」不代表已在三平台运行验收。Java 的 GTP 调用只作行为参考，新版 MVP 依照架构使用 KataGo analysis JSON 协议；GTP 对局后端仍在 P3。
- 同一能力的多个入口保留各自 ID，便于逐项定级；这些行数不是互不重复的产品能力数。动态引擎名称、最近文件、用户自定义快速启动程序不逐个实例计数。
- 搜索目录共 358 条：`FunctionCatalog.java` 直接声明 282 条（215 个菜单、44 个工具栏、21 个棋盘右键、2 个窗口条目），另由 `ConfigSettingTargets.java` 展开 76 条；以下逐 ID 列出。GUI、格式兼容和发布分发补充项另列，菜单栏及 36 个窗口/面板文件的导航索引见文末。

## 复盘与分析

| 功能（中文名） | 用户入口 | Java 主要源码 | 外部依赖（引擎 / 网络 / 平台） | 建议层级 | 理由（一句话） |
| --- | --- | --- | --- | --- | --- |
| 交换落子顺序（`toolbar.change-turn`） | 顶部工具栏 → 交换落子顺序 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 标记工具（`toolbar.markup-tools`） | 顶部工具栏 → 标记工具 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 字母标记（`toolbar.markup-letter`） | 顶部工具栏 → 字母标记 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 数字标记（`toolbar.markup-number`） | 顶部工具栏 → 数字标记 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 叉号标记（`toolbar.markup-cross`） | 顶部工具栏 → 叉号标记 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 方形标记（`toolbar.markup-square`） | 顶部工具栏 → 方形标记 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 三角标记（`toolbar.markup-triangle`） | 顶部工具栏 → 三角标记 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 橡皮擦（`toolbar.markup-eraser`） | 顶部工具栏 → 橡皮擦 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 清除标记（`toolbar.markup-clear`） | 顶部工具栏 → 清除标记 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 设置强制计算区域（`toolbar.force-allow`） | 顶部工具栏 → 设置强制计算区域 | `gui/Menu.java`；`gui/LizzieFrame.java` | 兼容选点限制 / 追踪的引擎；具体能力依引擎而异 | P2 | 限定或追踪研究点属于深入复盘，应在普通分析稳定后补齐。 |
| 设置强制不计算区域（`toolbar.force-avoid`） | 顶部工具栏 → 设置强制不计算区域 | `gui/Menu.java`；`gui/LizzieFrame.java` | 兼容 avoid 的引擎；Java 顶栏对 KataGo / Zen 禁用 | P3 | Java 的排除选点受引擎兼容限制，待后续后端确认等价语义。 |
| 清除强制区域（`toolbar.force-clear`） | 顶部工具栏 → 清除强制区域 | `gui/Menu.java`；`gui/LizzieFrame.java` | 兼容选点限制 / 追踪的引擎；具体能力依引擎而异 | P2 | 限定或追踪研究点属于深入复盘，应在普通分析稳定后补齐。 |
| 允许选点选项（`toolbar.force-allow-options`） | 顶部工具栏 → 允许选点选项 | `gui/Menu.java`；`gui/LizzieFrame.java` | 兼容选点限制 / 追踪的引擎；具体能力依引擎而异 | P2 | 限定或追踪研究点属于深入复盘，应在普通分析稳定后补齐。 |
| 排除选点选项（`toolbar.force-avoid-options`） | 顶部工具栏 → 排除选点选项 | `gui/Menu.java`；`gui/LizzieFrame.java` | 兼容 avoid 的引擎；Java 顶栏对 KataGo / Zen 禁用 | P3 | Java 的排除选点受引擎兼容限制，待后续后端确认等价语义。 |
| 选择分析黑白侧（`toolbar.analyze-sides`） | 顶部工具栏 → 分析: | `gui/Menu.java` | KataGo 或已存分析 | P2 | 按黑白侧筛选分析和候选信息属于核心分析之外的显示细化。 |
| 选择显示候选黑白侧（`toolbar.candidate-sides`） | 顶部工具栏 → 选点: | `gui/Menu.java` | KataGo 或已存分析 | P2 | 按黑白侧筛选分析和候选信息属于核心分析之外的显示细化。 |
| 候选点信息显示（`toolbar.candidate-info`） | 顶部工具栏 → 候选点信息显示 | `gui/Menu.java`；`gui/BoardRenderer.java` | KataGo 或已存分析 | MVP | 坐标、手数和分析数值让 P1 棋盘与胜率图可直接用于复盘。 |
| 向前十手（`review.previous-ten`） | 底部工具栏 → 向前十手 | `gui/BottomToolbar.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 逐手及分支导航直接支撑 P1 复盘主线；悔棋语义随 P3 对局提供。 |
| 向后十手（`review.next-ten`） | 底部工具栏 → 向后十手 | `gui/BottomToolbar.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 逐手及分支导航直接支撑 P1 复盘主线；悔棋语义随 P3 对局提供。 |
| 上一手（`review.previous`） | 底部工具栏 → 上一手 | `gui/BottomToolbar.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 逐手及分支导航直接支撑 P1 复盘主线；悔棋语义随 P3 对局提供。 |
| 下一手（`review.next`） | 底部工具栏 → 下一手 | `gui/BottomToolbar.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 逐手及分支导航直接支撑 P1 复盘主线；悔棋语义随 P3 对局提供。 |
| 跳转（`review.goto`） | 底部工具栏 → 跳转 | `gui/BottomToolbar.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 逐手及分支导航直接支撑 P1 复盘主线；悔棋语义随 P3 对局提供。 |
| 试下（`board.try-play`） | 底部工具栏 → 试下 | `gui/BottomToolbar.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 刷新当前分析并清引擎缓存（`board.refresh`） | 底部工具栏 → 刷新 | `gui/BottomToolbar.java`；`gui/LizzieFrame.java` | 支持 clear_cache 的运行中引擎；本地分析数据 | P2 | 清空当前分析并重新计算有助排除陈旧结果，新版按分析协议重设计缓存失效。 |
| 工具栏删除一手（`board.delete-without-prompt`） | 底部工具栏 → 删除 | `gui/BottomToolbar.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 候选点最高值红色高亮（`menu.showMaxValueReverse`） | 显示 → 选点信息 → 候选点最高值红色高亮 | `gui/Menu.java`；`gui/BoardRenderer.java` | 本地显示设置；数值部分依分析数据 | P2 | 候选点最高值红色高亮是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 清除编辑缓存（`board.clear-edit-cache`） | 主棋盘 → 右键单击空点 → 清除编辑缓存 | `gui/RightClickMenu.java`；`rules/Board.java` | 无；本地棋谱 | P2 | 保留编辑恢复与局面刷新能力，具体缓存操作随新版文档模型重设计。 |
| 跳到此步（`board.find-move`） | 主棋盘 → 右键单击棋子 → 跳到此步 | `gui/RightClickMenu2.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 逐手及分支导航直接支撑 P1 复盘主线；悔棋语义随 P3 对局提供。 |
| 插入黑子（`board.insert-black`） | 主棋盘 → 右键单击空点 → 插入黑子 | `gui/RightClickMenu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 插入白子（`board.insert-white`） | 主棋盘 → 右键单击空点 → 插入白子 | `gui/RightClickMenu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 只分析此点（`board.allow-point`） | 主棋盘 → 右键单击空点 → 只分析此点 | `gui/RightClickMenu.java`；`gui/LizzieFrame.java` | 兼容选点限制 / 追踪的引擎；具体能力依引擎而异 | P2 | 限定或追踪研究点属于深入复盘，应在普通分析稳定后补齐。 |
| 增加分析此点（`board.allow-more`） | 主棋盘 → 右键单击空点 → 增加分析此点 | `gui/RightClickMenu.java`；`gui/LizzieFrame.java` | 兼容选点限制 / 追踪的引擎；具体能力依引擎而异 | P2 | 限定或追踪研究点属于深入复盘，应在普通分析稳定后补齐。 |
| 去除分析此点（`board.allow-remove`） | 主棋盘 → 右键单击空点 → 去除分析此点 | `gui/RightClickMenu.java`；`gui/LizzieFrame.java` | 兼容选点限制 / 追踪的引擎；具体能力依引擎而异 | P2 | 限定或追踪研究点属于深入复盘，应在普通分析稳定后补齐。 |
| 不分析此点（`board.avoid-point`） | 主棋盘 → 右键单击空点 → 不分析此点 | `gui/RightClickMenu.java`；`gui/LizzieFrame.java` | 兼容 avoid 的引擎；Java 顶栏对 KataGo / Zen 禁用 | P3 | Java 的排除选点受引擎兼容限制，待后续后端确认等价语义。 |
| 持续分析/不分析（`board.keep-forcing`） | 主棋盘 → 右键单击空点 → 持续分析/不分析 | `gui/RightClickMenu.java`；`gui/LizzieFrame.java` | 兼容选点限制 / 追踪的引擎；具体能力依引擎而异 | P2 | 限定或追踪研究点属于深入复盘，应在普通分析稳定后补齐。 |
| 清除分析/不分析（`board.clear-forcing`） | 主棋盘 → 右键单击空点 → 清除分析/不分析 | `gui/RightClickMenu.java`；`gui/LizzieFrame.java` | 兼容选点限制 / 追踪的引擎；具体能力依引擎而异 | P2 | 限定或追踪研究点属于深入复盘，应在普通分析稳定后补齐。 |
| 评估此点（`board.track-point`） | 主棋盘 → 右键单击空点 → 评估此点 | `gui/RightClickMenu.java`；`gui/LizzieFrame.java` | 兼容选点限制 / 追踪的引擎；具体能力依引擎而异 | P2 | 限定或追踪研究点属于深入复盘，应在普通分析稳定后补齐。 |
| 取消评估（`board.untrack-point`） | 主棋盘 → 右键单击空点 → 取消评估 | `gui/RightClickMenu.java`；`gui/LizzieFrame.java` | 兼容选点限制 / 追踪的引擎；具体能力依引擎而异 | P2 | 限定或追踪研究点属于深入复盘，应在普通分析稳定后补齐。 |
| 清除所有评估选点（`board.clear-tracking`） | 主棋盘 → 右键单击空点 → 清除所有评估选点 | `gui/RightClickMenu.java`；`gui/LizzieFrame.java` | 兼容选点限制 / 追踪的引擎；具体能力依引擎而异 | P2 | 限定或追踪研究点属于深入复盘，应在普通分析稳定后补齐。 |
| 优先分析此点（`board.priority-point`） | 主棋盘 → 右键单击空点 → 优先分析此点 | `gui/RightClickMenu.java`；`gui/LizzieFrame.java` | 兼容选点限制 / 追踪的引擎；具体能力依引擎而异 | P2 | 限定或追踪研究点属于深入复盘，应在普通分析稳定后补齐。 |
| 清除优先分析（`board.clear-priority`） | 主棋盘 → 右键单击空点 → 清除优先分析 | `gui/RightClickMenu.java`；`gui/LizzieFrame.java` | 兼容选点限制 / 追踪的引擎；具体能力依引擎而异 | P2 | 限定或追踪研究点属于深入复盘，应在普通分析稳定后补齐。 |
| 添加为分支（`board.add-variation`） | 主棋盘 → 右键单击空点 → 添加为分支 | `gui/RightClickMenu.java`；`rules/Board.java` | KataGo 或已存候选变化；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 移动（`board.move-stone`） | 主棋盘 → 右键单击棋子 → 移动 | `gui/RightClickMenu2.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 反色（`board.switch-stone`） | 主棋盘 → 右键单击棋子 → 反色 | `gui/RightClickMenu2.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 删除（`board.delete-stone`） | 主棋盘 → 右键单击棋子 → 删除 | `gui/RightClickMenu2.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 回看（`board.review-stone`） | 主棋盘 → 右键单击棋子 → 回看 | `gui/RightClickMenu2.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 逐手及分支导航直接支撑 P1 复盘主线；悔棋语义随 P3 对局提供。 |
| 回退一手（`board.previous-or-regret`） | 主棋盘 → 右键单击空点 → 回退一手 | `gui/RightClickMenu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 逐手及分支导航直接支撑 P1 复盘主线；悔棋语义随 P3 对局提供。 |
| 新建（`files.new`） | 文件 → 新建 | `gui/Menu.java`、`gui/LizzieFrame.java`、`rules/Board.java` | 本地棋谱文档 | MVP | 棋谱读写与正确贴目是 P1 主线及 P2 SGF 兼容的基础。 |
| 打开棋谱（`files.open`） | 文件 → 打开棋谱 | `gui/Menu.java`；`rules/SGFParser.java` | 本地文件系统 | MVP | 棋谱读写与正确贴目是 P1 主线及 P2 SGF 兼容的基础。 |
| 保存（`files.save`） | 文件 → 保存 | `gui/Menu.java`；`rules/SGFParser.java` | 本地文件系统 | MVP | 棋谱读写与正确贴目是 P1 主线及 P2 SGF 兼容的基础。 |
| 另存为（`files.save-as`） | 文件 → 另存为 | `gui/Menu.java`；`rules/SGFParser.java` | 本地文件系统 | MVP | 棋谱读写与正确贴目是 P1 主线及 P2 SGF 兼容的基础。 |
| 保存纯净棋谱（`menu.saveRaw`） | 文件 → 更多保存 → 保存纯净棋谱 | `gui/Menu.java`；`rules/SGFParser.java` | 本地文件系统 / 系统剪贴板 | P2 | 为 SGF 编辑提供导出或临时交换方式，可在基本保存稳定后补齐。 |
| 保存纯净棋谱(带评论)（`menu.saveCommentRaw`） | 文件 → 更多保存 → 保存纯净棋谱(带评论) | `gui/Menu.java`；`rules/SGFParser.java` | 本地文件系统 / 系统剪贴板 | P2 | 为 SGF 编辑提供导出或临时交换方式，可在基本保存稳定后补齐。 |
| 保存纯净分支（`menu.saveBranchRaw`） | 文件 → 更多保存 → 保存纯净分支 | `gui/Menu.java`；`rules/SGFParser.java` | 本地文件系统 / 系统剪贴板 | P2 | 为 SGF 编辑提供导出或临时交换方式，可在基本保存稳定后补齐。 |
| 存档与读档（`menu.saveAndLoad`） | 文件 → 存档与读档 | `gui/Menu.java`；`rules/SGFParser.java` | 本地文件系统 / 系统剪贴板 | P2 | 为 SGF 编辑提供导出或临时交换方式，可在基本保存稳定后补齐。 |
| 复制棋谱到剪贴板（`menu.copySgf`） | 文件 → 复制棋谱到剪贴板 | `gui/Menu.java`；`rules/SGFParser.java` | 本地文件系统 / 系统剪贴板 | P2 | 为 SGF 编辑提供导出或临时交换方式，可在基本保存稳定后补齐。 |
| 从剪贴板粘贴棋谱（`menu.pasteSgf`） | 文件 → 从剪贴板粘贴棋谱 | `gui/Menu.java`；`rules/SGFParser.java` | 本地文件系统 / 系统剪贴板 | P2 | 为 SGF 编辑提供导出或临时交换方式，可在基本保存稳定后补齐。 |
| 加载 SGF 棋谱贴目（`menu.loadKomi`） | 文件 → 加载SGF棋谱中的贴目 | `gui/Menu.java`；`rules/SGFParser.java` | 本地文件系统 | MVP | 棋谱读写与正确贴目是 P1 主线及 P2 SGF 兼容的基础。 |
| 坐标（`menu.coordsMenu`） | 显示 → 坐标 | `gui/Menu.java`；`gui/BoardRenderer.java` | 无；本地棋盘 | MVP | 坐标、手数和分析数值让 P1 棋盘与胜率图可直接用于复盘。 |
| 手数（`menu.moveMenu`） | 显示 → 手数 | `gui/Menu.java`；`gui/BoardRenderer.java` | 无；本地棋谱 | MVP | 坐标、手数和分析数值让 P1 棋盘与胜率图可直接用于复盘。 |
| 落子评价标记（`menu.moveRankMenu`） | 显示 → 落子评价标记 | `gui/Menu.java`；`gui/BlunderListPanel.java` | KataGo 或已存分析 | MVP | 错误手标记与阈值服务于 P2 失误列表，具体样式可简化。 |
| 下一手提示（`menu.nextMoveHint`） | 显示 → 下一手提示 | `gui/Menu.java`；`gui/BoardRenderer.java` | 本地显示设置；数值部分依分析数据 | P2 | 下一手提示是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 自动播放（`menu.makeAutoPlay`） | 显示 → 自动播放 | `gui/Menu.java`；`gui/LizzieFrame.java` | 本地棋谱 / 已存变化 | P2 | 定时回放便于浏览变化，基本翻手可先覆盖复盘主线。 |
| 总是从1开始显示（`menu.moveNumberAlwaysFromOne`） | 显示 → 手数 → 总是从1开始显示 | `gui/Menu.java`；`gui/BoardRenderer.java` | 本地显示设置；数值部分依分析数据 | P2 | 总是从1开始显示是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 分支内从1开始显示（`menu.moveNumberInBranchFromOne`） | 显示 → 手数 → 分支内从1开始显示 | `gui/Menu.java`；`gui/BoardRenderer.java` | 本地显示设置；数值部分依分析数据 | P2 | 分支内从1开始显示是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 分支内总是显示所有手数（`menu.showAllMoveNumberInBranch`） | 显示 → 手数 → 分支内总是显示所有手数 | `gui/Menu.java`；`gui/BoardRenderer.java` | 本地显示设置；数值部分依分析数据 | P2 | 分支内总是显示所有手数是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 分支面板显示手数（`menu.showMoveNumberOnVariationPane`） | 显示 → 手数 → 显示分支面板上的手数 | `gui/Menu.java`；`gui/BoardRenderer.java` | 本地显示设置；数值部分依分析数据 | P2 | 分支面板显示手数是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 记录选点信息到评论中（`menu.appendWinrateToComment`） | 显示 → 主界面设置 → 记录选点信息到评论中 | `gui/Menu.java`；`rules/SGFParser.java` | KataGo 或已存分析；SGF 评论 | P2 | 将选点数值追加评论是 LZ 分析读写之外的可选导出形式。 |
| 棋盘下方显示黑白名字（`menu.showNameInBoard`） | 显示 → 主界面设置 → 棋盘下方显示黑白名字 | `gui/Menu.java`；`gui/BoardRenderer.java` | 本地显示设置；数值部分依分析数据 | P2 | 棋盘下方显示黑白名字是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 显示评论 / 问题手面板控制条（`menu.showCommentConrolPane`） | 显示 → 主界面设置 → 显示评论/问题手面板顶部控制条 | `gui/Menu.java`、`gui/SidebarPanel.java` | 本地显示设置；数值部分依分析数据 | P2 | 显示评论 / 问题手面板控制条是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 候选点显示胜率（`menu.suggestion1`） | 显示 → 选点信息 → 胜率 | `gui/Menu.java`；`gui/BoardRenderer.java` | KataGo 或已存分析 | MVP | 坐标、手数和分析数值让 P1 棋盘与胜率图可直接用于复盘。 |
| 候选点显示计算量（`menu.suggestion2`） | 显示 → 选点信息 → 计算量 | `gui/Menu.java`；`gui/BoardRenderer.java` | KataGo 或已存分析 | MVP | 坐标、手数和分析数值让 P1 棋盘与胜率图可直接用于复盘。 |
| 候选点显示目数（`menu.suggestion3`） | 显示 → 选点信息 → 目数 | `gui/Menu.java`；`gui/BoardRenderer.java` | KataGo 或已存分析 | MVP | 坐标、手数和分析数值让 P1 棋盘与胜率图可直接用于复盘。 |
| 显示目数为差值（`menu.showScoreAsDiff`） | 显示 → 选点信息 → 显示目数为差值 | `gui/Menu.java`；`gui/BoardRenderer.java` | 本地显示设置；数值部分依分析数据 | P2 | 显示目数为差值是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 自定义顺序（`menu.customInfoOrdr`） | 显示 → 选点信息 → 自定义顺序 | `gui/Menu.java`；`gui/BoardRenderer.java` | 本地显示设置；数值部分依分析数据 | P2 | 自定义顺序是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 延迟显示（`menu.setCandidatesDelay`） | 显示 → 选点信息 → 延迟显示 | `gui/Menu.java`；`gui/BoardRenderer.java` | 本地显示设置；数值部分依分析数据 | P2 | 延迟显示是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 小棋盘（`menu.subboard`） | 显示 → 面板 → 小棋盘 | `gui/Menu.java`；`gui/BoardRenderer.java` | 本地显示设置；数值部分依分析数据 | P2 | 小棋盘是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 胜率图（`menu.winrateGraph`） | 显示 → 面板 → 胜率图 | `gui/Menu.java`；`gui/WinrateGraph.java` | KataGo 或已存分析 | MVP | 坐标、手数和分析数值让 P1 棋盘与胜率图可直接用于复盘。 |
| 评论/问题手面板（`menu.commitPane`） | 显示 → 面板 → 评论/问题手面板 | `gui/Menu.java`；`gui/BlunderListPanel.java` | KataGo 分析数据 / SGF 评论 | MVP | 问题手与评论承载 P2 整盘复盘的主要输出。 |
| 分支面板（`menu.variationPane`） | 显示 → 面板 → 分支面板 | `gui/Menu.java`；`gui/VariationTree.java` | 无；本地棋谱 | MVP | 变化树面板是 ROADMAP P2 明确列出的复盘能力。 |
| 使用滚动面板拓展宽度（`menu.showScrollVariation`） | 显示 → 面板 → 分支面板设置 → 使用滚动面板拓展宽度 | `gui/Menu.java`、`gui/VariationTree.java`、`gui/LizzieFrame.java` | 本地棋谱变化树；桌面显示 | P2 | 使用滚动面板拓展宽度是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 设置最大宽度（`menu.maxTreeWidth`） | 显示 → 面板 → 分支面板设置 → 设置最大宽度 | `gui/Menu.java`、`gui/VariationTree.java`、`gui/LizzieFrame.java` | 本地棋谱变化树；桌面显示 | P2 | 设置最大宽度是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 变化树超宽后的滚动策略（`menu.ignoreOutOfWidth`） | 显示 → 面板 → 分支面板设置 → 超过宽度依然使用滚动条(超过部分将不显示) | `gui/Menu.java`、`gui/VariationTree.java`、`gui/LizzieFrame.java` | 本地棋谱变化树；桌面显示 | P2 | 变化树超宽后的滚动策略是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 选点列表面板（`menu.listPane`） | 显示 → 面板 → 选点列表面板 | `gui/Menu.java`、`gui/AnalysisTable.java` | KataGo 或已存分析 | MVP | 坐标、手数和分析数值让 P1 棋盘与胜率图可直接用于复盘。 |
| 超级鹰眼（`analysis.bad-moves`） | 显示 → 面板 → 超级鹰眼 | `gui/Menu.java`、`gui/MoveListFrame.java`、`gui/LizzieFrame.java` | 引擎分析结果；本地棋谱 | MVP | 失误列表属于 P2 复盘核心，旧独立窗口的高级统计可同阶段后补。 |
| 独立选点列表（`menu.SuggestionList`） | 显示 → 面板 → 独立选点列表 | `gui/Menu.java`、`gui/AnalysisFrame.java` | 本地显示设置；数值部分依分析数据 | P2 | 独立选点列表是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 显示选点右上方角标（`menu.showSuggestionOrder`） | 显示 → 选点信息 → 显示选点右上方角标 | `gui/Menu.java`；`gui/BoardRenderer.java` | 本地显示设置；数值部分依分析数据 | P2 | 显示选点右上方角标是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 轮白下时用白色显示候选文字（`menu.showWhiteSuggestWhite`） | 显示 → 选点信息 → 轮白下字体显示为白色 | `gui/Menu.java`；`gui/BoardRenderer.java` | 本地显示设置；数值部分依分析数据 | P2 | 轮白下时用白色显示候选文字是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 鼠标悬停显示变化图（`menu.showVariationOnMouse`） | 显示 → 选点信息 → 鼠标悬停显示变化图 | `gui/Menu.java`；`gui/BoardRenderer.java` | KataGo 或已存分析 | MVP | 候选变化预览解释分析推荐，支撑 P1 分析棋盘。 |
| 预览变化时冻结候选刷新（`menu.noRefreshOnMouse`） | 显示 → 选点信息 → 鼠标悬停显示变化图时,变化图不刷新 | `gui/Menu.java`；`gui/BoardRenderer.java` | 本地显示设置；数值部分依分析数据 | P2 | 预览变化时冻结候选刷新是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 显示胜率/目数曲线（`menu.showWinRateOrScoreLeadLine`） | 显示 → 胜率图设置 → 显示胜率/目数曲线 | `gui/Menu.java`；`gui/WinrateGraph.java` | KataGo 或已存分析 | MVP | 坐标、手数和分析数值让 P1 棋盘与胜率图可直接用于复盘。 |
| 单曲线模式使用面积填充（`menu.showWinrateGraphFill`） | 显示 → 胜率图设置 → 单曲线模式使用面积填充 | `gui/Menu.java`、`gui/WinrateGraph.java` | 本地显示设置；数值部分依分析数据 | P2 | 单曲线模式使用面积填充是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 显示柱状失误条（`menu.showBlunderBar`） | 显示 → 胜率图设置 → 显示柱状失误条 | `gui/Menu.java`、`gui/WinrateGraph.java` | KataGo 或已存分析 | MVP | 错误手标记与阈值服务于 P2 失误列表，具体样式可简化。 |
| 显示鼠标悬停信息(仅限黑视角)（`menu.showMouseOverWinrateGraph`） | 显示 → 胜率图设置 → 显示鼠标悬停信息(仅限黑视角) | `gui/Menu.java`、`gui/WinrateGraph.java` | 本地显示设置；数值部分依分析数据 | P2 | 显示鼠标悬停信息(仅限黑视角)是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 设置初始目数差最大值（`menu.initialMaxScoreLead`） | 显示 → 胜率图设置 → 设置初始目数差最大值 | `gui/Menu.java`、`gui/WinrateGraph.java` | 本地显示设置；数值部分依分析数据 | P2 | 设置初始目数差最大值是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 设置选点变化图回放间隔（`menu.setReplayInterval`） | 显示 → 选点信息 → 设置选点变化图回放间隔 | `gui/Menu.java`；`gui/LizzieFrame.java` | 本地棋谱 / 已存变化 | P2 | 定时回放便于浏览变化，基本翻手可先覆盖复盘主线。 |
| 热点图（`menu.heatMapSettings`） | 显示 → 小棋盘设置 → 热点图 | `gui/Menu.java`；`gui/BoardRenderer.java` | KataGo 或支持策略 / 所有权输出的引擎 | P2 | 策略与所有权可视化有助深入理解，先交付候选点和胜率目差。 |
| 变化图（`menu.subBoardShowVar`） | 显示 → 小棋盘设置 → 变化图 | `gui/Menu.java`；`gui/BoardRenderer.java` | 本地显示设置；数值部分依分析数据 | P2 | 变化图是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 目数显示（`menu.kataScolreLead`） | 显示 → KataGo相关设置 → 目数显示 | `gui/Menu.java`；`gui/BoardRenderer.java` | KataGo 或已存分析 | MVP | 坐标、手数和分析数值让 P1 棋盘与胜率图可直接用于复盘。 |
| Kata形势显示（`menu.kataEstimate`） | 显示 → KataGo相关设置 → Kata形势显示 | `gui/Menu.java`；`gui/BoardRenderer.java` | KataGo 或支持策略 / 所有权输出的引擎 | P2 | 策略与所有权可视化有助深入理解，先交付候选点和胜率目差。 |
| 记忆评估显示状态（`menu.kataEstimateSaveState`） | 显示 → KataGo相关设置 → Kata形势显示 → 记忆评估显示状态 | `gui/Menu.java`；`gui/BoardRenderer.java` | KataGo 或支持策略 / 所有权输出的引擎 | P2 | 策略与所有权可视化有助深入理解，先交付候选点和胜率目差。 |
| 以方块透明度表示占有率(小型)（`menu.kataEstimateByTransparentSmall`） | 显示 → KataGo相关设置 → Kata形势显示 → 以方块透明度表示占有率(小型) | `gui/Menu.java`；`gui/BoardRenderer.java` | KataGo 或支持策略 / 所有权输出的引擎 | P2 | 策略与所有权可视化有助深入理解，先交付候选点和胜率目差。 |
| 在纯网络中显示（`menu.kataEstimateInPureNet`） | 显示 → KataGo相关设置 → Kata形势显示 → 在纯网络中显示 | `gui/Menu.java`；`gui/BoardRenderer.java` | KataGo 或支持策略 / 所有权输出的引擎 | P2 | 策略与所有权可视化有助深入理解，先交付候选点和胜率目差。 |
| 按选点显示评估(最低版本1.11.0)（`menu.useMovesOwnership`） | 显示 → KataGo相关设置 → Kata形势显示 → 按选点显示评估(最低版本1.11.0) | `gui/Menu.java`；`gui/BoardRenderer.java` | KataGo 或支持策略 / 所有权输出的引擎 | P2 | 策略与所有权可视化有助深入理解，先交付候选点和胜率目差。 |
| 使用快捷键(大键盘点)（`menu.useKataEstimateShortcut`） | 显示 → KataGo相关设置 → Kata形势显示 → 使用快捷键(大键盘点) | `gui/Menu.java`；`gui/BoardRenderer.java` | KataGo 或支持策略 / 所有权输出的引擎 | P2 | 策略与所有权可视化有助深入理解，先交付候选点和胜率目差。 |
| 停一手（`menu.playPassMove`） | 棋局 → 停一手 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 起始局面设置模式（`menu.setupModeToggle`） | 棋局 → 起始局面设置 → 起始局面设置模式 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 清空全部设置子（`menu.setupClearAll`） | 棋局 → 起始局面设置 → 清空全部设置子 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 黑先（`menu.setupBlackToPlay`） | 棋局 → 起始局面设置 → 黑先 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 白先（`menu.setupWhiteToPlay`） | 棋局 → 起始局面设置 → 白先 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 将当前盘面设为起始局面...（`menu.convertCurrentPosition`） | 棋局 → 起始局面设置 → 将当前盘面设为起始局面... | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 开始/停止 分析（`analysis.toggle`） | 分析 → 开始/停止 分析 | `gui/Menu.java`；`analysis/EngineManager.java` | KataGo 与模型 | MVP | 实时分析的启停是 P1 必须贯通的核心操作。 |
| 自动分析（`auto-analysis`） | 分析 → 自动分析 | `gui/Menu.java`；`gui/StartAnaDialog.java` | KataGo 与模型 | MVP | 逐手自动复盘的能力进入 MVP，入口可与 P2 整盘分析合并。 |
| 批量分析（`batch-analysis`） | 分析 → 批量分析 | `gui/Menu.java`；`gui/LizzieFrame.java` | KataGo 与模型；本地棋谱 | P2 | 在整盘分析基础上补齐多文件、局部或分支分析的效率功能。 |
| 批量精析（`batch-deep-analysis`） | 分析 → 批量精析 | `gui/Menu.java`；`gui/LizzieFrame.java` | KataGo 与模型；本地棋谱 | P2 | 在整盘分析基础上补齐多文件、局部或分支分析的效率功能。 |
| 停止自动(批量)分析（`stop-analysis`） | 分析 → 停止自动(批量)分析 | `gui/Menu.java`；`gui/BottomToolbar.java` | KataGo 与模型 | MVP | 自动分析必须可取消；新版与整盘任务共享取消入口，批量场景随扩展提供。 |
| 批量分析进度表（`batch-progress`） | 分析 → 批量分析进度表 | `gui/Menu.java`；`gui/LizzieFrame.java` | KataGo 与模型；本地棋谱 | P2 | 在整盘分析基础上补齐多文件、局部或分支分析的效率功能。 |
| 局部精析（`partial-lightning-analysis`） | 分析 → 局部精析 | `gui/Menu.java`；`gui/LizzieFrame.java` | KataGo 与模型；本地棋谱 | P2 | 在整盘分析基础上补齐多文件、局部或分支分析的效率功能。 |
| 全部分支精析（`all-branches-lightning-analysis`） | 分析 → 全部分支精析 | `gui/Menu.java`；`gui/LizzieFrame.java` | KataGo 与模型；本地棋谱 | P2 | 在整盘分析基础上补齐多文件、局部或分支分析的效率功能。 |
| 精析设置（`lightning-analysis-settings`） | 分析 → 精析设置 | `gui/Menu.java`；`gui/LizzieFrame.java` | KataGo 与模型；本地棋谱 | P2 | 在整盘分析基础上补齐多文件、局部或分支分析的效率功能。 |
| 纯网络（`menu.showHeatmap`） | 分析 → 纯网络 | `gui/Menu.java`；`gui/BoardRenderer.java` | KataGo 或支持策略 / 所有权输出的引擎 | P2 | 策略与所有权可视化有助深入理解，先交付候选点和胜率目差。 |
| 策略网络（`menu.showPolicy`） | 分析 → 策略网络 | `gui/Menu.java`；`gui/BoardRenderer.java` | KataGo 或支持策略 / 所有权输出的引擎 | P2 | 策略与所有权可视化有助深入理解，先交付候选点和胜率目差。 |
| 清除分析信息(全局)（`menu.clearAllLizzieBestmoves`） | 分析 → 清除分析信息(全局) | `gui/Menu.java`；`rules/Board.java` | 本地棋谱 / 分析缓存 | MVP | 清理过期分析数据是 SGF 分析兼容与缓存管理的必要补充。 |
| 清除分析信息(此手)（`menu.clearThisLizzieBestmoves`） | 分析 → 清除分析信息(此手) | `gui/Menu.java`；`rules/Board.java` | 本地棋谱 / 分析缓存 | MVP | 清理过期分析数据是 SGF 分析兼容与缓存管理的必要补充。 |
| 添加黑子（`menu.addBlack`） | 编辑 → 添加黑子 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 添加白子（`menu.addWhite`） | 编辑 → 添加白子 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 交替落子（`menu.alternatelyMoves`） | 编辑 → 交替落子 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 启用双击找子（`menu.allowDoubleClick`） | 编辑 → 启用双击找子 | `gui/Menu.java`；`rules/Board.java` | 无；桌面输入 | P2 | 为复盘导航或棋谱编辑提供可选手势，基本命令可先独立交付。 |
| 启用拖动棋子（`menu.allowDrag`） | 编辑 → 启用拖动棋子 | `gui/Menu.java`；`rules/Board.java` | 无；桌面输入 | P2 | 为复盘导航或棋谱编辑提供可选手势，基本命令可先独立交付。 |
| 启用点击棋子回看（`menu.allowClickReview`） | 编辑 → 启用点击棋子回看 | `gui/Menu.java`；`rules/Board.java` | 无；桌面输入 | P2 | 为复盘导航或棋谱编辑提供可选手势，基本命令可先独立交付。 |
| 插入黑子（`menu.insertBlack`） | 编辑 → 插入黑子 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 插入白子（`menu.insertWhite`） | 编辑 → 插入白子 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 清空棋盘（`menu.clearBoard`） | 编辑 → 清空棋盘 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 返回主分支（`menu.backToMainBranch`） | 编辑 → 返回主分支 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 逐手及分支导航直接支撑 P1 复盘主线；悔棋语义随 P3 对局提供。 |
| 设为主分支（`menu.setAsMain`） | 编辑 → 设为主分支 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 跳转到最前（`menu.jumpToFirst`） | 编辑 → 跳转到最前 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 逐手及分支导航直接支撑 P1 复盘主线；悔棋语义随 P3 对局提供。 |
| 跳转到最后（`menu.jumpToLast`） | 编辑 → 跳转到最后 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 逐手及分支导航直接支撑 P1 复盘主线；悔棋语义随 P3 对局提供。 |
| 跳转到左分支（`menu.jumpToLeft`） | 编辑 → 跳转到左分支 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 逐手及分支导航直接支撑 P1 复盘主线；悔棋语义随 P3 对局提供。 |
| 跳转到右分支（`menu.jumpToRight`） | 编辑 → 跳转到右分 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 逐手及分支导航直接支撑 P1 复盘主线；悔棋语义随 P3 对局提供。 |
| 删除一手（`menu.delete`） | 编辑 → 删除一手 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 删除分支（`menu.deleteBranch`） | 编辑 → 删除分支 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 撤销删除(可在右键菜单中操作)（`menu.undoDelete`） | 编辑 → 撤销删除(可在右键菜单中操作) | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 恢复删除(可在右键菜单中操作)（`menu.redoDelete`） | 编辑 → 恢复删除(可在右键菜单中操作) | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 编辑棋局信息与贴目（`game.komi`） | 编辑 → 编辑棋局信息 → 贴目 | `gui/Menu.java`、`gui/GameInfoDialog.java`、`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 设置棋盘大小（`menu.setBoard`） | 编辑 → 设置棋盘大小 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 交换黑白（`menu.exchange`） | 编辑 → 交换黑白 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | P2 | 棋谱对称变换便于研究，优先级低于无损读写与分支编辑。 |
| 向右旋转（`menu.spinRight`） | 编辑 → 向右旋转 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | P2 | 棋谱对称变换便于研究，优先级低于无损读写与分支编辑。 |
| 向左旋转（`menu.spinLeft`） | 编辑 → 向左旋转 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | P2 | 棋谱对称变换便于研究，优先级低于无损读写与分支编辑。 |
| 水平翻转（`menu.mirrorVertical`） | 编辑 → 水平翻转 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | P2 | 棋谱对称变换便于研究，优先级低于无损读写与分支编辑。 |
| 垂直翻转（`menu.mirrorHorizon`） | 编辑 → 垂直翻转 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | P2 | 棋谱对称变换便于研究，优先级低于无损读写与分支编辑。 |
| 最近打开（`files.recent`） | 文件 → 最近打开 | `gui/Menu.java`；`rules/SGFParser.java` | 本地文件系统 | MVP | 棋谱读写与正确贴目是 P1 主线及 P2 SGF 兼容的基础。 |
| 下一手提示的低计算量阈值（`menu.minPlayoutsForNextMove`） | 显示 → 下一手提示 → 设置低计算量阈值（当前值动态显示） | `gui/Menu.java`；`gui/BoardRenderer.java` | 本地显示设置；数值部分依分析数据 | P2 | 下一手提示的低计算量阈值是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 整盘精析（推荐）（`whole-game-deep-analysis`） | 分析 → 整盘精析（推荐） | `gui/Menu.java`；`gui/WholeGameAnalysisDialog.java` | KataGo 与模型 | MVP | 整盘分析与可取消任务是 ROADMAP P2 明确要求。 |
| 整盘速览（闪电）（`whole-game-lightning-overview`） | 分析 → 整盘速览（闪电） | `gui/Menu.java`；`gui/LizzieFrame.java` | KataGo 与模型 | MVP | 整盘分析与可取消任务是 ROADMAP P2 明确要求。 |
| 放置黑子（`menu.setupToolBlack`） | 棋局 → 起始局面设置 → 放置黑子 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 放置白子（`menu.setupToolWhite`） | 棋局 → 起始局面设置 → 放置白子 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 擦除棋子（`menu.setupToolErase`） | 棋局 → 起始局面设置 → 擦除棋子 | `gui/Menu.java`；`rules/Board.java` | 无；本地棋谱 | MVP | 属于 P2 SGF 编辑、起始局面与变化树的基本操作。 |
| 打开棋谱后自动快速分析（`config.kifu.auto-analyze`） | 设置 → 综合设置 → 棋谱加载 → 打开棋谱后自动快速分析 | `gui/ConfigDialog2.java` | 本地棋谱；自动分析需 KataGo | P2 | 打开棋谱后的自动动作属于偏好，先保证手动打开与分析可用。 |
| 打开后跳到最后一手（`config.kifu.jump-last`） | 设置 → 综合设置 → 棋谱加载 → 打开后跳到最后一手 | `gui/ConfigDialog2.java` | 本地棋谱 | P2 | 打开棋谱后的自动动作属于偏好，先保证手动打开与分析可用。 |
| 读取棋谱贴目（`config.kifu.read-komi`） | 设置 → 综合设置 → 棋谱加载 → 读取棋谱贴目 | `gui/ConfigDialog2.java`；`rules/SGFParser.java` | 本地文件系统 | MVP | 棋谱读写与正确贴目是 P1 主线及 P2 SGF 兼容的基础。 |
| 显示胜率曲线（`config.analysis.winrate`） | 设置 → 综合设置 → 引擎与分析 → 显示胜率曲线 | `gui/ConfigDialog2.java`；`gui/WinrateGraph.java` | KataGo 或已存分析 | MVP | 坐标、手数和分析数值让 P1 棋盘与胜率图可直接用于复盘。 |
| 总是显示黑胜率/目数（`settings.black-winrate`） | 设置 → 综合设置 → 引擎与分析 → 总是显示黑胜率/目数 | `gui/ConfigDialog2.java` | KataGo 或已存分析 | MVP | 新版架构固定黑方视角，因此保留语义并取消可切换视角的必要性。 |
| 显示分支面板（`config.analysis.variation`） | 设置 → 综合设置 → 引擎与分析 → 显示分支面板 | `gui/ConfigDialog2.java`；`gui/VariationTree.java` | 无；本地棋谱 | MVP | 变化树面板是 ROADMAP P2 明确列出的复盘能力。 |
| 显示柱状失误条（`config.analysis.blunder-bar`） | 设置 → 综合设置 → 引擎与分析 → 显示柱状失误条 | `gui/ConfigDialog2.java`、`gui/WinrateGraph.java` | 引擎或已存的逐手分析结果 | MVP | 问题手与评论承载 P2 整盘复盘的主要输出。 |
| 鼠标悬停胜率图（`config.analysis.hover`） | 设置 → 综合设置 → 引擎与分析 → 鼠标悬停胜率图 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 鼠标悬停胜率图是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 单曲线模式使用面积填充（`config.analysis.graph-fill`） | 设置 → 综合设置 → 引擎与分析 → 单曲线模式使用面积填充 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 单曲线模式使用面积填充是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 候选点最高值红色高亮（`config.analysis.max-red`） | 设置 → 综合设置 → 引擎与分析 → 候选点最高值红色高亮 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 候选点最高值红色高亮是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 选点重点研究累计计算量目标（`config.analysis.tracking-visits`） | 设置 → 综合设置 → 引擎与分析 → 选点重点研究累计计算量目标 | `gui/ConfigDialog2.java`；`gui/LizzieFrame.java` | 兼容选点限制 / 追踪的引擎；具体能力依引擎而异 | P2 | 限定或追踪研究点属于深入复盘，应在普通分析稳定后补齐。 |
| 选点数量上限（`config.analysis.suggestion-limit`） | 设置 → 综合设置 → 引擎与分析 → 选点数量上限 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 选点数量上限是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 变化图长度上限（`config.analysis.variation-limit`） | 设置 → 综合设置 → 引擎与分析 → 变化图长度上限 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 变化图长度上限是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 最佳选点显示蓝圈（`config.candidates.blue-ring`） | 设置 → 综合设置 → 引擎与分析 → 最佳选点显示蓝圈 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 最佳选点显示蓝圈是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 选点颜色集中程度（`config.candidates.color-ratio`） | 设置 → 综合设置 → 引擎与分析 → 选点颜色集中程度 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 选点颜色集中程度是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 轮白下的选点颜色（`config.candidates.white-style`） | 设置 → 综合设置 → 引擎与分析 → 轮白下的选点颜色 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 轮白下的选点颜色是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 超限选点仍显示圆圈（`config.candidates.no-suggestion-circle`） | 设置 → 综合设置 → 引擎与分析 → 超限选点仍显示圆圈 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 超限选点仍显示圆圈是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 隐藏低于该比例的低计算量选点（`config.candidates.minimum-playout-ratio`） | 设置 → 综合设置 → 引擎与分析 → 隐藏低于该比例的低计算量选点 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 隐藏低于该比例的低计算量选点是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 显示 PV 访问次数（`config.pv.mode`） | 设置 → 综合设置 → 引擎与分析 → 显示 PV 访问次数 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 显示 PV 访问次数是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| PV 访问次数阈值（`config.pv.visit-limit`） | 设置 → 综合设置 → 引擎与分析 → PV 访问次数阈值 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | PV 访问次数阈值是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 变化图中去掉死子（`config.pv.remove-dead`） | 设置 → 综合设置 → 引擎与分析 → 变化图中去掉死子 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 变化图中去掉死子是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 启用双击找子（`config.play.double-click`） | 设置 → 综合设置 → 对局与操作 → 启用双击找子 | `gui/ConfigDialog2.java`；`rules/Board.java` | 无；桌面输入 | P2 | 为复盘导航或棋谱编辑提供可选手势，基本命令可先独立交付。 |
| 启用点击复盘（`config.play.click-review`） | 设置 → 综合设置 → 对局与操作 → 启用点击复盘 | `gui/ConfigDialog2.java`；`rules/Board.java` | 无；桌面输入 | P2 | 为复盘导航或棋谱编辑提供可选手势，基本命令可先独立交付。 |
| 启用拖拽棋子（`config.play.drag-stone`） | 设置 → 综合设置 → 对局与操作 → 启用拖拽棋子 | `gui/ConfigDialog2.java`；`rules/Board.java` | 无；桌面输入 | P2 | 为复盘导航或棋谱编辑提供可选手势，基本命令可先独立交付。 |
| 显示评论/问题手面板（`config.play.comment-panel`） | 设置 → 综合设置 → 对局与操作 → 显示评论/问题手面板 | `gui/ConfigDialog2.java`；`gui/BlunderListPanel.java` | KataGo 分析数据 / SGF 评论 | MVP | 问题手与评论承载 P2 整盘复盘的主要输出。 |
| 隐藏面板顶部控制条（`config.play.hide-panel-controls`） | 设置 → 综合设置 → 对局与操作 → 隐藏面板顶部控制条 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 隐藏面板顶部控制条是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 显示坐标（`config.play.coordinates`） | 设置 → 综合设置 → 对局与操作 → 显示坐标 | `gui/ConfigDialog2.java`；`gui/BoardRenderer.java` | 无；本地棋盘 | MVP | 坐标、手数和分析数值让 P1 棋盘与胜率图可直接用于复盘。 |
| 小棋盘不跟随刷新（`config.play.freeze-sub-board`） | 设置 → 综合设置 → 对局与操作 → 小棋盘不跟随刷新 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 小棋盘不跟随刷新是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 显示落子矩形（`config.interaction.move-rectangle`） | 设置 → 综合设置 → 对局与操作 → 显示落子矩形 | `gui/ConfigDialog2.java` | 无；桌面输入反馈 | P2 | 显示落子矩形是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 右键行为（`config.interaction.right-click`） | 设置 → 综合设置 → 对局与操作 → 右键行为 | `gui/ConfigDialog2.java`；`rules/Board.java` | 无；桌面输入 | P2 | 为复盘导航或棋谱编辑提供可选手势，基本命令可先独立交付。 |
| 坐标格式（`config.interaction.coordinates`） | 设置 → 综合设置 → 对局与操作 → 坐标格式 | `gui/ConfigDialog2.java` | 无；本地坐标显示 | P2 | 坐标格式是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 显示小棋盘（`config.display.sub-board`） | 设置 → 综合设置 → 棋盘与显示 → 显示小棋盘 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 显示小棋盘是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 窗口标题显示胜率（`config.display.title-winrate`） | 设置 → 综合设置 → 棋盘与显示 → 窗口标题显示胜率 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 窗口标题显示胜率是复盘展示的细化选项，可在基本棋盘和分析输出之后补齐。 |
| 胜率曲线宽度（`config.theme.winrate-width`） | 设置 → 综合设置 → 主题外观 → 胜率曲线宽度 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 调整胜率曲线宽度属于复盘显示偏好，默认清晰样式先随 MVP 交付。 |
| 柱状失误条最小宽度（`config.theme.blunder-width`） | 设置 → 综合设置 → 主题外观 → 柱状失误条最小宽度 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 调整柱状失误条最小宽度属于复盘显示偏好，默认清晰样式先随 MVP 交付。 |
| 胜率曲线颜色（`config.theme.winrate-color`） | 设置 → 综合设置 → 主题外观 → 胜率曲线颜色 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 调整胜率曲线颜色属于复盘显示偏好，默认清晰样式先随 MVP 交付。 |
| 胜率缺失曲线颜色（`config.theme.missing-color`） | 设置 → 综合设置 → 主题外观 → 胜率缺失曲线颜色 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 调整胜率缺失曲线颜色属于复盘显示偏好，默认清晰样式先随 MVP 交付。 |
| 胜率变化条颜色（`config.theme.blunder-color`） | 设置 → 综合设置 → 主题外观 → 胜率变化条颜色 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 调整胜率变化条颜色属于复盘显示偏好，默认清晰样式先随 MVP 交付。 |
| 目数曲线颜色（`config.theme.score-color`） | 设置 → 综合设置 → 主题外观 → 目数曲线颜色 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 调整目数曲线颜色属于复盘显示偏好，默认清晰样式先随 MVP 交付。 |
| 评论背景色（`config.theme.comment-background`） | 设置 → 综合设置 → 主题外观 → 评论背景色 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 调整评论背景色属于复盘显示偏好，默认清晰样式先随 MVP 交付。 |
| 评论字体色（`config.theme.comment-text`） | 设置 → 综合设置 → 主题外观 → 评论字体色 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 调整评论字体色属于复盘显示偏好，默认清晰样式先随 MVP 交付。 |
| 第一选点颜色（`config.theme.best-move-color`） | 设置 → 综合设置 → 主题外观 → 第一选点颜色 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 调整第一选点颜色属于复盘显示偏好，默认清晰样式先随 MVP 交付。 |
| 棋子标志类型（`config.theme.stone-indicator`） | 设置 → 综合设置 → 主题外观 → 棋子标志类型 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 调整棋子标志类型属于复盘显示偏好，默认清晰样式先随 MVP 交付。 |
| 显示评论节点颜色（`config.theme.comment-node-color`） | 设置 → 综合设置 → 主题外观 → 显示评论节点颜色 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 调整显示评论节点颜色属于复盘显示偏好，默认清晰样式先随 MVP 交付。 |
| 显示关注点质量外框（`config.tracking.outline`） | 设置 → 综合设置 → 主题外观 → 显示关注点质量外框 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 调整显示关注点质量外框属于复盘显示偏好，默认清晰样式先随 MVP 交付。 |
| 外框不透明度（`config.tracking.outline-opacity`） | 设置 → 综合设置 → 主题外观 → 外框不透明度 | `gui/ConfigDialog2.java` | 本地显示设置；数值部分依分析数据 | P2 | 调整外框不透明度属于复盘显示偏好，默认清晰样式先随 MVP 交付。 |
| 错误节点规则（`config.theme.blunder-rules`） | 设置 → 综合设置 → 主题外观 → 错误节点规则 | `gui/ConfigDialog2.java`；`gui/BlunderListPanel.java` | KataGo 或已存分析 | MVP | 错误手标记与阈值服务于 P2 失误列表，具体样式可简化。 |
| 同时考虑胜率与目数（`config.theme.score-blunders`） | 设置 → 综合设置 → 主题外观 → 同时考虑胜率与目数 | `gui/ConfigDialog2.java`；`gui/BlunderListPanel.java` | KataGo 或已存分析 | MVP | 错误手标记与阈值服务于 P2 失误列表，具体样式可简化。 |
| 评论与问题手侧栏切换、黑白筛选及跳转（`extra.gui.problem-sidebar`） | 主窗口侧栏「评论 / 问题手」、点击或键盘选择问题手 | `gui/SidebarPanel.java`、`gui/SidebarHeaderPanel.java`、`gui/BlunderListPanel.java`、`gui/LizzieFrame.java` | KataGo 分析结果；离线 | MVP | 将整盘分析结果直接连到问题手导航，属于 P2 明确的失误列表。 |
| 棋谱评论查看与编辑（`extra.gui.comment-edit`） | 主窗口评论面板、评论编辑控件 | `gui/SidebarPanel.java`、`gui/LizzieFrame.java` | 本地 SGF | MVP | 评论是 P2 棋谱编辑的基础内容。 |
| 超级鹰眼分阶段统计与筛选（`extra.gui.review-statistics`） | 显示 → 超级鹰眼；选择全局 / 开局 / 中盘 / 终盘 / 自定义区间 | `gui/MoveListFrame.java` | 已有引擎分析结果；离线 | P2 | 基础失误列表先进入 MVP，复杂符合率与分阶段统计可作为同阶段增补。 |
| 复盘表格导出（`extra.gui.review-table-export`） | 超级鹰眼表格右键 → 导出 | `gui/MoveListFrame.java`、`util/Utils.java` | 本地文件；`.xls` 导出 | P2 | 导出便于教练整理结果，但不阻塞最小复盘闭环。 |
| 选点列表与计算量图、下一手及悬停切换（`extra.gui.candidate-window`） | 显示 → 选点列表；窗口顶部选项 | `gui/AnalysisFrame.java`、`gui/LizzieFrame.java` | KataGo；副引擎模式另需第二引擎 | P2 | 主棋盘候选点为基础，独立统计窗口是复盘增补。 |
| 整盘精析（推荐）的计算量预设、进度、暂停和取消（`extra.gui.whole-game-controls`） | 分析 → 整盘精析（推荐） | `gui/WholeGameAnalysisDialog.java` | KataGo analysis 引擎 | MVP | P2 整盘分析需要用户可控的预算和进度反馈。 |
| 逐手自动分析的范围、黑白方、差值重算与自动保存（`extra.gui.auto-analysis-options`） | 分析 → 自动分析；底部详细工具栏 | `gui/StartAnaDialog.java`、`gui/BottomToolbar.java` | Java 逐手分析引擎；本地 SGF | P2 | 新版先提供整盘分析，旧版逐手工作流的细项再按使用反馈补齐。 |
| GIB 棋谱导入 `extra.files.gib` | 文件 → 打开，选择 GIB；拖入棋谱 | `rules/GIBParser.java`、`gui/LizzieFrame.java` | 本地 GIB 文件；无需联网或引擎解析 | MVP | ROADMAP P2 明确要求与弈城历史棋谱互通。 |
| 棋谱中文编码识别与 UTF-8 保存 `extra.files.encoding` | 打开及保存 SGF，导入 GIB | `rules/SGFParser.java`、`rules/GIBParser.java`、`util/EncodingDetector.java` | 本地文件；Java 使用 UniversalDetector，WINDOWS-1252 检测结果回退 GB18030 | MVP | 旧棋谱能正确显示姓名和注释是 P1/P2 迁移主线所需。 |
| SGF 的 LZ 分析数据读写 `extra.files.lz` | 打开带分析的 SGF；文件 → 保存 | `rules/SGFParser.java`、`rules/SgfAnalysisPayloadWriter.java` | 本地 SGF；产生新分析需受支持引擎（新版采用 KataGo），读取已有值不需引擎 | MVP | ROADMAP P2 明确要求保留 Java 分析结果以支持并行使用。 |
| SGF 快照与双引擎分析扩展（LZOP / LZ2 / LZOP2） `extra.files.lz-extensions` | 打开或保存含 LZ2 / LZOP / LZOP2 的棋谱 | `rules/SGFParser.java`、`rules/SgfAnalysisPayloadWriter.java` | 对应旧棋谱；实时双引擎需额外算力 | P3 | MVP 原样保留这些属性，根局面快照及双槽分析的完整语义交互暂按 P3 建议待审。 |

## 引擎与设置

| 功能（中文名） | 用户入口 | Java 主要源码 | 外部依赖（引擎 / 网络 / 平台） | 建议层级 | 理由（一句话） |
| --- | --- | --- | --- | --- | --- |
| 更新官方权重（`weights.download`） | 设置 → 引擎一键设置 → 权重管理 → 官方权重 | `gui/KataGoAutoSetupDialog.java` | 网络；KataGo 模型下载服务 | MVP | 引擎向导、权重和兼容运行时组件属于 ROADMAP P2 分发要求。 |
| 英伟达显卡提速组件（`engine.acceleration`） | 设置 → 引擎一键设置 → 英伟达显卡提速组件 | `gui/KataGoAutoSetupDialog.java` | 网络；KataGo / 模型；提速组件限受支持 NVIDIA 平台 | MVP | 引擎向导、权重和兼容运行时组件属于 ROADMAP P2 分发要求。 |
| 打开 PDA 选项（`engine.pda`） | 顶部工具栏 → 打开 PDA 选项 | `gui/Menu.java`；`gui/SetKataPDA.java` | KataGo 与模型；部分通用参数依引擎 | P2 | PDA、搜索广度等专业调参在最小引擎配置后补齐。 |
| 分析广度拓展（WRN）（`toolbar.wrn`） | 顶部工具栏 → 分析广度拓展,仅分析时有效,点击"参数"了解更多信息 | `gui/Menu.java`；`gui/EngineParameter.java` | KataGo 与模型；部分通用参数依引擎 | P2 | PDA、搜索广度等专业调参在最小引擎配置后补齐。 |
| 分析时间上限（`toolbar.time-limit`） | 顶部工具栏 → 时间 | `gui/Menu.java` | KataGo 与模型 | MVP | 基本计算预算控制是持续使用本地分析的必要设置。 |
| 分析计算量上限（`toolbar.visit-limit`） | 顶部工具栏 → 计算 | `gui/Menu.java` | KataGo 与模型 | MVP | 基本计算预算控制是持续使用本地分析的必要设置。 |
| Gtp控制台（`menu.gtpPanel`） | 显示 → 面板 → Gtp控制台 | `gui/Menu.java`；`gui/LizzieFrame.java` | GTP 引擎 | P3 | GTP 控制台随 ROADMAP P3 的 GTP 后端保留，P1 使用 JSON 分析协议。 |
| 清除Lizzie缓存(全局)（`menu.clearAllLizzieCache`） | 分析 → 清除Lizzie缓存(全局) | `gui/Menu.java`；`rules/Board.java` | 本地分析缓存 | MVP | 分析复用与失效处理对应 P2 缓存目标；Java 原有缓存不等同于新版 SQLite。 |
| 清除Lizzie缓存(此手)（`menu.clearThisLizzieCache`） | 分析 → 清除Lizzie缓存(此手) | `gui/Menu.java`；`rules/Board.java` | 本地分析缓存 | MVP | 分析复用与失效处理对应 P2 缓存目标；Java 原有缓存不等同于新版 SQLite。 |
| 引擎（`engine.configuration`） | 设置 → 引擎 | `gui/Menu.java`；`analysis/EngineManager.java` | 本地引擎进程与模型 | MVP | 配置、选择和恢复主引擎是 P1 本地分析运行闭环。 |
| 引擎规则（`engine.rules`） | 设置 → 引擎规则 → 胜负判断: | `gui/Menu.java`；`gui/SetKataRules.java` | KataGo | MVP | 规则必须与棋谱及 KataGo 请求一致，属于正确分析的基础。 |
| 引擎参数（`engine.parameters`） | 设置 → 引擎参数 | `gui/Menu.java`；`gui/EngineParameter.java` | KataGo 与模型；部分通用参数依引擎 | P2 | PDA、搜索广度等专业调参在最小引擎配置后补齐。 |
| 引擎一键设置（`engine.setup`） | 设置 → 引擎一键设置 | `gui/Menu.java`；`gui/KataGoAutoSetupDialog.java` | 网络；KataGo / 模型；操作系统与 CPU / GPU 后端 | MVP | 引擎向导、权重和兼容运行时组件属于 ROADMAP P2 分发要求。 |
| 综合设置（`settings.general`） | 设置 → 综合设置 | `gui/Menu.java`；`gui/ConfigDialog2.java` | 本地配置 | MVP | 基础设置是 P2 明确要求；细粒度选项在下方逐项定级。 |
| 选择主引擎（`engine.select`） | 选择主引擎 | `gui/Menu.java`；`analysis/EngineManager.java` | 本地引擎进程与模型 | MVP | 配置、选择和恢复主引擎是 P1 本地分析运行闭环。 |
| 选择对比引擎（`engine.select-secondary`） | 选择对比引擎（双引擎布局） | `gui/Menu.java`；`analysis/EngineManager.java` | 多个引擎进程 / 模型；对比需双引擎模式 | P3 | 多引擎并行及对比需要额外资源调度，不在 P1 单焦点主线内。 |
| 重启当前引擎（`menu.restartCurrentEngine`） | 选择主引擎 → 重启当前引擎 | `gui/Menu.java`；`analysis/EngineManager.java` | 本地引擎进程与模型 | MVP | 配置、选择和恢复主引擎是 P1 本地分析运行闭环。 |
| 关闭当前引擎（`menu.shutdownCurrentEngine`） | 选择主引擎 → 关闭引擎 → 关闭当前引擎 | `gui/Menu.java`；`analysis/EngineManager.java` | 本地引擎进程与模型 | MVP | 配置、选择和恢复主引擎是 P1 本地分析运行闭环。 |
| 关闭当前以外引擎（`menu.shutdownOtherEngine`） | 选择主引擎 → 关闭引擎 → 关闭当前以外引擎 | `gui/Menu.java`；`analysis/EngineManager.java` | 多个引擎进程 / 模型；对比需双引擎模式 | P3 | 多引擎并行及对比需要额外资源调度，不在 P1 单焦点主线内。 |
| 关闭所有引擎（`menu.shutdownAllEngine`） | 选择主引擎 → 关闭引擎 → 关闭所有引擎 | `gui/Menu.java`；`analysis/EngineManager.java` | 多个引擎进程 / 模型；对比需双引擎模式 | P3 | 多引擎并行及对比需要额外资源调度，不在 P1 单焦点主线内。 |
| 重启对比引擎（`menu.restartCurrentEngine2`） | 选择对比引擎 → 重启当前引擎 | `gui/Menu.java`；`analysis/EngineManager.java` | 多个引擎进程 / 模型；对比需双引擎模式 | P3 | 多引擎并行及对比需要额外资源调度，不在 P1 单焦点主线内。 |
| 关闭对比引擎（`menu.shutdownCurrentEngine2`） | 选择对比引擎 → 关闭当前引擎 | `gui/Menu.java`；`analysis/EngineManager.java` | 多个引擎进程 / 模型；对比需双引擎模式 | P3 | 多引擎并行及对比需要额外资源调度，不在 P1 单焦点主线内。 |
| 限制分析时长（`config.analysis.limit-time`） | 设置 → 综合设置 → 引擎与分析 → 限制分析时长 | `gui/ConfigDialog2.java`；`gui/Menu.java` | KataGo 与模型 | MVP | 基本计算预算控制是持续使用本地分析的必要设置。 |
| 限制分析计算量（`config.analysis.limit-visits`） | 设置 → 综合设置 → 引擎与分析 → 限制分析计算量 | `gui/ConfigDialog2.java`；`gui/Menu.java` | KataGo 与模型 | MVP | 基本计算预算控制是持续使用本地分析的必要设置。 |
| 启用引擎快速切换（`config.advanced.fast-switch`） | 设置 → 综合设置 → 高级选项 → 启用引擎快速切换 | `gui/ConfigDialog2.java`；`analysis/EngineManager.java` | 多个引擎进程 / 模型；对比需双引擎模式 | P3 | 多引擎并行及对比需要额外资源调度，不在 P1 单焦点主线内。 |
| 启用 Lizzie 缓存（`config.advanced.cache`） | 设置 → 综合设置 → 高级选项 → 启用 Lizzie 缓存 | `gui/ConfigDialog2.java`；`rules/Board.java` | 本地分析缓存 | MVP | 分析复用与失效处理对应 P2 缓存目标；Java 原有缓存不等同于新版 SQLite。 |
| 空棋盘停止计算（`config.advanced.stop-empty`） | 设置 → 综合设置 → 高级选项 → 空棋盘停止计算 | `gui/ConfigDialog2.java`；`analysis/EngineManager.java` | KataGo 与受支持硬件 | P2 | 节能与首次测速是引擎向导的体验增补，可晚于基础引擎可用。 |
| 首次启动智能测速（`config.advanced.startup-benchmark`） | 设置 → 综合设置 → 高级选项 → 首次启动智能测速 | `gui/ConfigDialog2.java`；`gui/KataGoAutoSetupDialog.java` | KataGo 与受支持硬件 | P2 | 节能与首次测速是引擎向导的体验增补，可晚于基础引擎可用。 |
| 控制台隐藏时仍记录 GTP（`config.engine.always-gtp`） | 设置 → 综合设置 → 高级选项 → 控制台隐藏时仍记录 GTP | `gui/ConfigDialog2.java`；`gui/LizzieFrame.java` | GTP 引擎 | P3 | GTP 控制台随 ROADMAP P3 的 GTP 后端保留，P1 使用 JSON 分析协议。 |
| 自动检测引擎是否存活（`config.engine.check-alive`） | 设置 → 综合设置 → 高级选项 → 自动检测引擎是否存活 | `gui/ConfigDialog2.java`；`analysis/EngineManager.java` | 本地引擎进程 | MVP | 引擎故障发现与分析更新频率直接关系 P1 可靠性。 |
| 本地分析刷新间隔（`config.engine.update-interval`） | 设置 → 综合设置 → 高级选项 → 本地分析刷新间隔 | `gui/ConfigDialog2.java`；`analysis/EngineManager.java` | 本地引擎进程 | MVP | 引擎故障发现与分析更新频率直接关系 P1 可靠性。 |
| 选择本地 KataGo、路径诊断与修复分析配置（`extra.gui.engine-repair`） | 设置 → 引擎一键设置 → 概览 | `gui/KataGoAutoSetupDialog.java` | 本地 KataGo 可执行文件、模型、配置；平台架构 | MVP | 引擎向导须能恢复缺少路径或分析配置的常见故障。 |
| 自定义权重导入、本地权重选择与切换（`extra.gui.local-weights`） | 一键设置 → 权重管理 → 自定义 / 已下载权重 | `gui/KataGoAutoSetupDialog.java` | 本地 KataGo 与兼容权重 | MVP | 与官方权重下载共同组成 P2 引擎向导的基本使用路径。 |
| 轻量快速分析模型的按需下载与启用（`extra.gui.quick-model`） | 一键设置 → 权重管理 → 快速分析模型 | `gui/KataGoAutoSetupDialog.java` | KataGo；模型下载网络 | P2 | 整盘分析可先复用现有模型，专用轻量模型属于性能增补。 |
| HumanSL 模型下载与导入（`extra.gui.humansl-assets`） | 一键设置 → 权重管理 → HumanSL 模型；AI 陪练准备页 | `gui/KataGoAutoSetupDialog.java`、`gui/NewHumanSlGameDialog.java` | 支持 HumanSL 的 KataGo、模型；下载网络 | P3 | 模型服务于路线图 P3 的陪练和棋力评估。 |
| 性能优化测速、推理与搜索速度展示（`extra.gui.engine-benchmark`） | 一键设置 → 性能优化 | `gui/KataGoAutoSetupDialog.java`、`gui/B11SpeedNoticePanel.java` | 本地 KataGo、对应 GPU / CPU 后端 | P2 | 基础引擎能用后再补测速，避免以速度数字代替棋力选择。 |
| 实测报告导入、应用场景参数与恢复原配置（`extra.gui.measured-tuning`） | 一键设置 → 性能优化 → 导入实测报告 / 恢复原配置 | `gui/MeasuredTuningDialog.java`、`gui/KataGoAutoSetupDialog.java`、`util/MeasuredKataGoTuning.java` | 本地 JSON 测量报告、匹配的 KataGo 条目 | P3 | 高级调优需要可核对的机器实测，优先级低于引擎向导。 |
| 实验后端安装及 Apple Silicon 实验性能调优（`extra.gui.experimental-backends`） | 一键设置 → 加速组件 / 性能优化中的实验选项 | `gui/KataGoAutoSetupDialog.java` | 特定 GPU / NPU、系统驱动和运行时；组件下载网络 | 候选砍掉 | 保留按需组件的可能性，暂不为实验后端承担默认发行成本。 |
| 第三方 GTP 引擎的可视化棋力设置（`extra.gui.gtp-strength-config`） | 设置 → 引擎 → 支持该能力的条目 → 棋力设置 | `gui/GtpEngineConfigDialog.java`、`gui/MoreEngines.java` | 提供配置 schema 的 GTP 引擎；旧 SSH 不支持自动检测 | P3 | 依赖路线图 P3 的第三方 GTP 后端。 |
| 引擎与权重目录、完整性和运行时兼容检查 `extra.engine.distribution` | 设置 → 引擎一键设置 → 安装引擎 / 权重 | `util/KataGoAssetCatalog.java`、`util/KataGoAutoSetupHelper.java`、`src/main/resources/katago-assets.json` | 发布源网络；SHA-256；CPU/GPU 后端及对应运行时 | MVP | ROADMAP P2 要求共用分发组件，下载并加载的资源必须可校验。 |
| 多个实验 GPU 完整分发包 `extra.engine.experimental-packages` | 发布下载页选择 DirectML / OpenVINO / ROCm 变体 | `util/KataGoAssetCatalog.java`；`scripts/package_windows_exe.sh`、`scripts/package_katago_source_windows.py` | Windows；指定 GPU/NPU 架构、驱动与实验后端 | 候选砍掉 | 保留硬件支持的评估入口，但优先研究按需组件而非承诺维护每种完整包。 |
| Leela Zero 专用分析适配 `extra.engine.leela-zero` | 设置 → 引擎 → 配置并选择 Leela Zero 命令 | `gui/MoreEngines.java`、`analysis/Leelaz.java` | Leela Zero 可执行文件与权重；GTP / lz-analyze 扩展 | 候选砍掉 | 路线图明确将 Leela Zero 专用适配列为候选取舍，不与 P3 通用 GTP 后端绑定保留。 |

## 对局与训练

| 功能（中文名） | 用户入口 | Java 主要源码 | 外部依赖（引擎 / 网络 / 平台） | 建议层级 | 理由（一句话） |
| --- | --- | --- | --- | --- | --- |
| 暂停当前对局（`toolbar.game-pause`） | 顶部工具栏 → 暂停 | `gui/Menu.java`；`gui/LizzieFrame.java` | 引擎 / 模型；Genmove 模式依 GTP | P3 | 人机对局及其用时控制明确属于 ROADMAP P3。 |
| 结束对局（`toolbar.game-stop`） | 顶部工具栏 → 结束对局 | `gui/Menu.java`；`gui/LizzieFrame.java` | 引擎 / 模型；Genmove 模式依 GTP | P3 | 人机对局及其用时控制明确属于 ROADMAP P3。 |
| 认输（`toolbar.game-resign`） | 顶部工具栏 → 认输 | `gui/Menu.java`；`gui/LizzieFrame.java` | 引擎 / 模型；Genmove 模式依 GTP | P3 | 人机对局及其用时控制明确属于 ROADMAP P3。 |
| 详细工具栏：引擎对弈设置（`legacy.engine-game`） | 底部工具栏 → 详细工具栏：引擎对弈设置（旧详细工具栏限中文） | `gui/BottomToolbar.java`；`gui/NewEngineGameDialog.java` | 两个兼容对局引擎 / 模型 | P3 | 引擎对局与批量赛程控制明确属于 ROADMAP P3。 |
| 详细工具栏：开始或停止引擎对弈（`legacy.engine-game-start`） | 底部工具栏 → 详细工具栏：开始或停止引擎对弈（旧详细工具栏限中文） | `gui/BottomToolbar.java`；`gui/NewEngineGameDialog.java` | 两个兼容对局引擎 / 模型 | P3 | 引擎对局与批量赛程控制明确属于 ROADMAP P3。 |
| 详细工具栏：引擎对弈高级设置（`legacy.engine-game-settings`） | 底部工具栏 → 详细工具栏：引擎对弈高级设置（旧详细工具栏限中文） | `gui/BottomToolbar.java`；`gui/EnginePkConfig.java` | 两个兼容对局引擎 / 模型 | P3 | 引擎对局与批量赛程控制明确属于 ROADMAP P3。 |
| 详细工具栏：暂停或继续引擎对弈（`legacy.engine-game-pause`） | 底部工具栏 → 详细工具栏：暂停或继续引擎对弈（旧详细工具栏限中文） | `gui/BottomToolbar.java`；`gui/NewEngineGameDialog.java` | 两个兼容对局引擎 / 模型 | P3 | 引擎对局与批量赛程控制明确属于 ROADMAP P3。 |
| 详细工具栏：介入引擎对弈（`legacy.engine-game-intervene`） | 底部工具栏 → 详细工具栏：介入引擎对弈（旧详细工具栏限中文） | `gui/BottomToolbar.java`；`gui/Manual.java` | 两个兼容对局引擎 / 模型 | P3 | 引擎对局与批量赛程控制明确属于 ROADMAP P3。 |
| 独立棋局控制面板（`menu.ctrlPanel`） | 显示 → 面板 → 独立棋局控制面板 | `gui/Menu.java`；`gui/LizzieFrame.java` | 桌面窗口系统；对局引擎 | P3 | 独立棋局控制面板随 P3 对局与多窗口工作流提供。 |
| 人机续弈（`menu.continueGameAgainstAi`） | 棋局 → 人机续弈 | `gui/Menu.java`；`gui/LizzieFrame.java` | 引擎 / 模型；Genmove 模式依 GTP | P3 | 人机对局及其用时控制明确属于 ROADMAP P3。 |
| 人机对局（`menu.newGenmoveGame`） | 棋局 → 新对局 → 人机对局 | `gui/Menu.java`；`gui/NewGameDialog.java` | 引擎 / 模型；Genmove 模式依 GTP | P3 | 人机对局及其用时控制明确属于 ROADMAP P3。 |
| 引擎对局（`menu.newEngineGame`） | 棋局 → 新对局 → 引擎对局 | `gui/Menu.java`；`gui/NewEngineGameDialog.java` | 两个兼容对局引擎 / 模型 | P3 | 引擎对局与批量赛程控制明确属于 ROADMAP P3。 |
| AI 陪练（`menu.newHumanSlGame`） | 棋局 → 新对局 → AI 陪练 | `gui/Menu.java`；`gui/NewHumanSlGameDialog.java` | KataGo 与 HumanSL 模型 | P3 | HumanSL 陪练明确属于 ROADMAP P3。 |
| 终局点目（`menu.scoreGame`） | 棋局 → 终局点目 | `gui/Menu.java`；`gui/LizzieFrame.java` | 本地棋盘；引擎辅助依当前模式 | P3 | 终局点目先随对局工作流提供，复盘 MVP 先显示引擎目差。 |
| 修改AI用时（`menu.setAiTime`） | 棋局 → 修改AI用时 | `gui/Menu.java`；`gui/SetAiTimes.java` | 引擎 / 模型；Genmove 模式依 GTP | P3 | 人机对局及其用时控制明确属于 ROADMAP P3。 |
| 终止人机对局（`game.stop-human`） | 棋局 → 终止人机对局 | `gui/Menu.java`；`gui/LizzieFrame.java` | 引擎 / 模型；Genmove 模式依 GTP | P3 | 人机对局及其用时控制明确属于 ROADMAP P3。 |
| 终止引擎对局（`menu.breakEngineGame`） | 棋局 → 终止引擎对局 | `gui/Menu.java`；`gui/NewEngineGameDialog.java` | 两个兼容对局引擎 / 模型 | P3 | 引擎对局与批量赛程控制明确属于 ROADMAP P3。 |
| 暂停/继续引擎对局（`menu.pauseEngineGame`） | 棋局 → 暂停/继续引擎对局 | `gui/Menu.java`；`gui/NewEngineGameDialog.java` | 两个兼容对局引擎 / 模型 | P3 | 引擎对局与批量赛程控制明确属于 ROADMAP P3。 |
| 修改多盘引擎对局数（`menu.changeEngineGameNumbers`） | 棋局 → 修改多盘引擎对局数 | `gui/Menu.java`；`gui/NewEngineGameDialog.java` | 两个兼容对局引擎 / 模型 | P3 | 引擎对局与批量赛程控制明确属于 ROADMAP P3。 |
| 干预引擎对局（`menu.intervention`） | 棋局 → 干预引擎对局 | `gui/Menu.java`；`gui/Manual.java` | 两个兼容对局引擎 / 模型 | P3 | 引擎对局与批量赛程控制明确属于 ROADMAP P3。 |
| 让AI落子最佳一手（`menu.playBestMove`） | 棋局 → 让AI落子最佳一手 | `gui/Menu.java`；`gui/LizzieFrame.java` | 引擎 / 模型；Genmove 模式依 GTP | P3 | 人机对局及其用时控制明确属于 ROADMAP P3。 |
| 自动补完征子（`menu.continueLadder`） | 棋局 → 自动补完征子 | `gui/Menu.java`；`rules/Board.java` | 本地棋盘 / 征子逻辑 | P3 | 自动补征子是专项训练辅助，未列入复盘 MVP。 |
| AI 解说（`analysis.commentary`） | 分析 → AI 解说 | `gui/Menu.java`、`teacher/TeacherDialog.java` | KataGo；所选解说服务可能需要网络 / API 凭据 | P3 | AI 解说明确属于 ROADMAP P3，先保证数值分析可信。 |
| 死活题（`game.tsumego`） | 分析 → 死活题 | `gui/Menu.java`；`gui/TsumeGoFrame.java` | KataGo；抓题依屏幕采集能力 | P3 | 死活题及采集明确属于 ROADMAP P3。 |
| 抓取死活题（`menu.captureTsumeGo`） | 分析 → 抓取死活题 | `gui/Menu.java`；`gui/CaptureTsumeGoFrame.java` | KataGo；抓题依屏幕采集能力 | P3 | 死活题及采集明确属于 ROADMAP P3。 |
| 棋力水平评估（`menu.playerStrengthEstimate`） | 分析 → 棋力水平评估 | `gui/Menu.java`；`gui/StrengthDialog.java` | KataGo / 评估模型与分析数据 | P3 | 棋力评估明确属于 ROADMAP P3。 |
| 显示跑谱贡献菜单（`menu.showContribute`） | 设置 → 显示跑谱贡献菜单 | `gui/Menu.java` | 本地配置；具体贡献功能另需网络与引擎 | P3 | 分布式自我对弈贡献是独立工作流，复盘主线稳定后再评估。 |
| 可视化KataGo分布式训练（`menu.kataGoDistributedTraining`） | 跑谱贡献 → 可视化KataGo分布式训练 | `gui/Menu.java` | 网络；KataGo contribute 与服务账号 | P3 | 分布式自我对弈贡献是独立工作流，复盘主线稳定后再评估。 |
| KataGo训练设置（`menu.kataGoTrainingSettings`） | 跑谱贡献 → KataGo训练设置 | `gui/Menu.java` | 网络；KataGo contribute 与服务账号 | P3 | 分布式自我对弈贡献是独立工作流，复盘主线稳定后再评估。 |
| KataGo官方网站（`menu.kataGoOfficialWebsite`） | 跑谱贡献 → KataGo官方网站 | `gui/Menu.java` | 网络；系统浏览器 / KataGo 训练网站 | P3 | 训练网站是贡献工作流的外部链接，随该功能按需提供。 |
| AI 陪练（`game.ai-coach`） | AI 陪练 | `gui/Menu.java`；`gui/NewHumanSlGameDialog.java` | KataGo 与 HumanSL 模型 | P3 | HumanSL 陪练明确属于 ROADMAP P3。 |
| 人机对局（分析模式）（`menu.newAnalyzeModeGame`） | 棋局 → 新对局 → 人机对局（分析模式） | `gui/Menu.java`；`gui/NewAnaGameDialog.java` | 引擎 / 模型；Genmove 模式依 GTP | P3 | 人机对局及其用时控制明确属于 ROADMAP P3。 |
| 对局时后台计算（`config.advanced.ponder`） | 设置 → 综合设置 → 高级选项 → 对局时后台计算 | `gui/ConfigDialog2.java`；`gui/LizzieFrame.java` | 引擎 / 模型；Genmove 模式依 GTP | P3 | 人机对局及其用时控制明确属于 ROADMAP P3。 |
| 五子棋无提子规则（`config.advanced.no-capture`） | 设置 → 综合设置 → 高级选项 → 五子棋无提子规则 | `gui/ConfigDialog2.java`；`rules/Board.java` | 无；本地棋盘规则 | 候选砍掉 | 五子棋规则偏离围棋复盘核心，需独立需求证明后再考虑。 |
| 人机对局计时、让子、续弈与自动保存（`extra.gui.human-game-options`） | 棋局 → 新对局 → Genmove / 分析模式对局 | `gui/NewGameDialog.java`、`gui/NewAnaGameDialog.java` | 支持对应模式的引擎；本地棋谱 | P3 | 人机对局属于 P3，连同计时和让子规则一起设计。 |
| 引擎比赛批量局数、开局棋谱、认输和双边用时（`extra.gui.engine-match-options`） | 棋局 → 新对局 → 引擎对局；底部详细工具栏 | `gui/NewEngineGameDialog.java`、`gui/BottomToolbar.java` | 两个引擎；本地开局 SGF | P3 | 批量比赛依附 P3 引擎对局，不纳入复盘 MVP。 |
| 对局中分引擎调整 WRN / PDA（`extra.gui.engine-game-parameters`） | 引擎对局工具条 → 对局参数；引擎对局控制菜单 | `gui/EngineGameParametersDialog.java`、`gui/EngineGameParameterControls.java` | 正在进行的引擎对局；后端需支持对应参数 | P3 | 只对引擎比赛有意义，随 P3 对局参数能力补齐。 |
| 比赛规则快照详情（`extra.gui.match-rules-details`） | 对局棋盘的规则说明区域；规则入口 | `gui/MatchRulesDetailsDialog.java`、`gui/LizzieFrame.java` | 当前或已保存的比赛规则快照；无需再次查询引擎 | P3 | 用于区分比赛约定与实际引擎规则，随比赛功能交付。 |
| AI 陪练提示、悔棋、问题手重下、重试与赛后复盘（`extra.gui.coach-actions`） | 菜单栏 AI 陪练 → 陪练侧栏 / 问题手提示卡 | `gui/CoachPanel.java`、`gui/NewHumanSlGameDialog.java` | KataGo、HumanSL 模型及陪练控制器 | P3 | 属于路线图明示的 HumanSL 陪练体验。 |
| 棋力评估模型选择、阶段划分与 HumanSL 准备（`extra.gui.strength-models`） | 分析 → 棋力评估 → 模型 / 阶段 / 准备 | `gui/StrengthDialog.java`、`gui/HumanSlStrengthDialog.java`、`gui/StrengthDialogHost.java` | KataGo 分析结果；HumanSL 模型可按需下载 | P3 | 棋力估算与模型解释需要独立验证，不能挤入复盘 MVP。 |
| 死活题轮走方、攻方、劫材与围墙设置（`extra.gui.tsumego-options`） | 分析 → 死活题；截取死活题窗口 → 设置 | `gui/TsumeGoFrame.java`、`gui/CaptureTsumeGoFrame.java` | KataGo；截图操作需要桌面捕获能力 | P3 | 这是路线图 P3 死活题功能的配置子集。 |

## 在线与同步

| 功能（中文名） | 用户入口 | Java 主要源码 | 外部依赖（引擎 / 网络 / 平台） | 建议层级 | 理由（一句话） |
| --- | --- | --- | --- | --- | --- |
| 强制结束试下（`sync.exit-web-trial`） | 底部工具栏 → 强制结束试下 | `gui/BottomToolbar.java`；`gui/web/WebBoardManager.java` | 本地 HTTP / WebSocket 服务；浏览器 / 局域网 | P3 | 网页棋盘与旁观试下明确属于 ROADMAP P3。 |
| 打开在线链接（`files.url`） | 文件 → 打开在线链接 | `gui/Menu.java`；`gui/OnlineDialog.java` | 网络；受支持的棋谱 URL 服务 | P3 | 通用在线链接依各棋谱服务适配，MVP 先覆盖明确的野狐入口。 |
| 弈客直播（`menu.yikeLive`） | 同步 → 弈客直播 | `gui/Menu.java`；`gui/YikeLiveDialog.java` | 网络；弈客服务 | P3 | 弈客及实时同步明确列入 ROADMAP P3。 |
| 打开弈客网页版（`menu.yikeLiveWeb`） | 同步 → 打开弈客网页版 | `gui/Menu.java`；`gui/BrowserFrame.java` | 网络；弈客网页 / 浏览器 | 候选砍掉 | 内嵌网页增加浏览器维护面，可评估改为系统浏览器链接。 |
| 弈客大厅（`menu.yikeRoom`） | 同步 → 弈客大厅 | `gui/Menu.java`、`gui/BrowserFrame.java`、`gui/OnlineDialog.java` | 网络；弈客网页；JCEF 原生运行时 | 候选砍掉 | 内嵌大厅依赖浏览器运行时，可评估改为外部浏览器并保留 P3 同步需求。 |
| 野狐棋谱(输入野狐昵称获取)（`menu.foxKifu`） | 同步 → 野狐棋谱(输入野狐昵称获取) | `gui/Menu.java`；`gui/FoxKifuDownload.java` | 网络；野狐棋谱接口 | MVP | 野狐昵称抓谱明确列入 ROADMAP P2。 |
| 腾讯棋谱（`menu.tencentKifu`） | 同步 → 腾讯棋谱 | `gui/Menu.java`；`gui/TencentKifuDownload.java` | 网络；腾讯棋谱接口 | P3 | 腾讯棋谱服务不在 P2 已确认的野狐昵称抓谱范围。 |
| 棋盘同步工具（`sync.board`） | 同步 → 棋盘同步工具 | `gui/Menu.java`；`analysis/ReadBoard.java` | Windows；readboard 本地组件 / 目标棋盘 | P3 | readboard 桥接明确属于 ROADMAP P3。 |
| 启动 Web 旁观（`menu.webBoardToggle`） | 同步 → Web 旁观 → 启动 Web 旁观 | `gui/Menu.java`；`gui/web/WebBoardManager.java` | 本地 HTTP / WebSocket 服务；浏览器 / 局域网 | P3 | 网页棋盘与旁观试下明确属于 ROADMAP P3。 |
| 复制访问地址（`menu.webBoardCopyUrl`） | 同步 → Web 旁观 → 复制访问地址 | `gui/Menu.java`；`gui/web/WebBoardManager.java` | 本地 HTTP / WebSocket 服务；浏览器 / 局域网 | P3 | 网页棋盘与旁观试下明确属于 ROADMAP P3。 |
| 允许弈客直播/大厅进入对局室（`menu.EnableEnterYikeGame`） | 同步 → 允许弈客直播/大厅进入对局室 | `gui/Menu.java`；`gui/YikeLiveDialog.java` | 网络；弈客服务 | P3 | 弈客及实时同步明确列入 ROADMAP P3。 |
| 有新的一手时总是跳转到最新一手（`menu.alwaysGoToLastMove`） | 同步 → 有新的一手时总是跳转到最新一手 | `gui/Menu.java`；`gui/YikeLiveDialog.java` | 网络；弈客服务 | P3 | 弈客及实时同步明确列入 ROADMAP P3。 |
| 棋盘同步工具选项（`sync.settings`） | 同步 → 棋盘同步工具选项 | `gui/Menu.java`；`analysis/ReadBoard.java` | Windows；readboard 本地组件 / 目标棋盘 | P3 | readboard 桥接明确属于 ROADMAP P3。 |
| 总是保持棋盘一致(回退时可能破坏历史手顺)（`menu.alwaysKeepBoardStatSync`） | 同步 → 棋盘同步工具选项 → 总是保持棋盘一致(回退时可能破坏历史手顺) | `gui/Menu.java`；`analysis/ReadBoard.java` | Windows；readboard 本地组件 / 目标棋盘 | P3 | readboard 桥接明确属于 ROADMAP P3。 |
| 滚轮控制变化图时自动获取焦点（`menu.readBoardGetFocus`） | 同步 → 棋盘同步工具选项 → 滚轮控制变化图时自动获取焦点 | `gui/Menu.java`；`analysis/ReadBoard.java` | Windows；readboard 本地组件 / 目标棋盘 | P3 | readboard 桥接明确属于 ROADMAP P3。 |
| 远程算力（`engine.remote`） | 设置 → 远程算力 | `gui/Menu.java`；`gui/RemoteComputeDialog.java` | 网络；远端引擎；WebSocket / SSH / 智子服务 | P3 | 远程算力及 SSH 明确列入 ROADMAP P3。 |
| 同步时不播放落子声（`menu.notPlaySoundInSync`） | 设置 → 同步时不播放落子声 | `gui/Menu.java` | 在线同步 / 本地音频 | P3 | 同步场景的声音选项随 P3 在线同步实现。 |
| 网络代理（`config.proxy.mode`） | 设置 → 综合设置 → 高级选项 → 网络代理 | `gui/ConfigDialog2.java`；`util/NetworkProxy.java` | HTTP / SOCKS 代理（手动模式需代理地址） | P2 | 网络代理有助于组件与棋谱下载，但可在基础网络流程后补齐。 |
| 手动代理地址（`config.proxy.manual-address`） | 设置 → 综合设置 → 高级选项 → 手动代理地址 | `gui/ConfigDialog2.java`；`util/NetworkProxy.java` | HTTP / SOCKS 代理（手动模式需代理地址） | P2 | 网络代理有助于组件与棋谱下载，但可在基础网络流程后补齐。 |
| SSH 分析刷新间隔（`config.engine.ssh-update-interval`） | 设置 → 综合设置 → 高级选项 → SSH 分析刷新间隔 | `gui/ConfigDialog2.java`；`gui/RemoteComputeDialog.java` | 网络；远端引擎；WebSocket / SSH / 智子服务 | P3 | 远程算力及 SSH 明确列入 ROADMAP P3。 |
| 智子登录、验证码、密码重置及安全记忆凭据（`extra.gui.remote-login`） | 设置 → 远程算力 → 智子 | `gui/RemoteComputeDialog.java` | 智子网络服务；系统凭据存储 | P3 | 远程算力及账号生命周期属于 P3。 |
| 自建算力链接 / 二维码导入与切回本机（`extra.gui.remote-custom`） | 设置 → 远程算力 → 自建算力 / 切回本机 | `gui/RemoteComputeDialog.java` | 远程 WebSocket KataGo、网络；可选本地二维码图片 | P3 | 保持远程连接独立于本地复盘 MVP。 |
| 智子用量、余额与流水明细（`extra.gui.remote-account`） | 远程算力 → 智子账号 → 用量明细 / 收支明细 | `gui/ZhiziAccountDetailsDialog.java`、`gui/RemoteComputeDialog.java` | 智子账号与网络 API | P3 | 服务账号信息随远程算力功能一起补齐。 |
| 智子充值、VIP 购买及支付状态（`extra.gui.remote-payment`） | 远程算力 → 充值 / 开通 VIP | `gui/ZhiziVipPurchaseDialog.java`、`gui/RemoteComputeDialog.java` | 智子商品 / 订单服务、扫码支付网络 | P3 | 付费服务入口不属于本地复盘所需能力。 |
| 弈客直播分类、筛选、翻页与当前对局同步（`extra.gui.yike-live-list`） | 同步 → 弈客直播；底部直播按钮 | `gui/YikeLiveDialog.java`、`gui/OnlineDialog.java` | 弈客网络 API / SGF；分析需 KataGo | P3 | 路线图把弈客集成明确安排在 P3。 |
| 内嵌网页浏览器与弈客网页同步（`extra.gui.embedded-browser`） | 同步 → 弈客直播网页版 / 弈客大厅 | `gui/BrowserFrame.java`、`gui/OnlineDialog.java` | JCEF 原生运行时、远端网页；系统架构 | 候选砍掉 | 路线图允许降级内嵌浏览器，可优先保留外部浏览器或独立同步能力。 |
| 野狐 AI 直连 `extra.sync.fox-direct` | 同步 → 野狐 AI 直连 | `gui/Menu.java`、`fox/FoxLiveSession.java`、`fox/FoxConnection.java`、`analysis/FoxEngineSession.java` | 127.0.0.1 TCP 桥接客户端、对局引擎及外部野狐服务 | P3 | 这是实时对局桥接，区别于 P2 的野狐昵称抓谱，应随对局与同步评估。 |

## 工具

| 功能（中文名） | 用户入口 | Java 主要源码 | 外部依赖（引擎 / 网络 / 平台） | 建议层级 | 理由（一句话） |
| --- | --- | --- | --- | --- | --- |
| 自由涂鸦（`toolbar.painting`） | 顶部工具栏 → 自由涂鸦 | `gui/Menu.java` | 桌面绘图 | P3 | 自由涂鸦是讲棋辅助，MVP 先支持标准 SGF 标记。 |
| 详细工具栏：自动分析设置（`legacy.auto-analysis`） | 底部工具栏 → 详细工具栏：自动分析设置（旧详细工具栏限中文） | `gui/BottomToolbar.java` | 依对应分析 / 自动播放功能；旧入口限中文 | 候选砍掉 | 保留对应分析能力，合并中文专属旧工具栏重复入口以降低维护成本。 |
| 详细工具栏：开始或停止自动分析（`legacy.auto-analysis-start`） | 底部工具栏 → 详细工具栏：开始或停止自动分析（旧详细工具栏限中文） | `gui/BottomToolbar.java` | 依对应分析 / 自动播放功能；旧入口限中文 | 候选砍掉 | 保留对应分析能力，合并中文专属旧工具栏重复入口以降低维护成本。 |
| 详细工具栏：自动落子设置（`legacy.auto-play`） | 底部工具栏 → 详细工具栏：自动落子设置（旧详细工具栏限中文） | `gui/BottomToolbar.java` | 依对应分析 / 自动播放功能；旧入口限中文 | 候选砍掉 | 保留对应分析能力，合并中文专属旧工具栏重复入口以降低维护成本。 |
| 自动播放(大)(秒)（`legacy.auto-main`） | 底部工具栏 → 自动播放(大)(秒)（旧详细工具栏限中文） | `gui/BottomToolbar.java` | 依对应分析 / 自动播放功能；旧入口限中文 | 候选砍掉 | 保留对应分析能力，合并中文专属旧工具栏重复入口以降低维护成本。 |
| 自动变化图(毫秒)（`legacy.auto-sub`） | 底部工具栏 → 自动变化图(毫秒)（旧详细工具栏限中文） | `gui/BottomToolbar.java` | 依对应分析 / 自动播放功能；旧入口限中文 | 候选砍掉 | 保留对应分析能力，合并中文专属旧工具栏重复入口以降低维护成本。 |
| 保存主棋盘截图（`menu.saveMainBoardScreen`） | 文件 → 更多保存 → 保存主棋盘截图 | `gui/Menu.java`；`gui/LizzieFrame.java` | 文件系统 / 系统剪贴板；桌面渲染 | P2 | 图片分享提升复盘结果传播，优先级低于棋谱和分析数据保存。 |
| 保存小棋盘截图（`menu.saveSubBoardScreen`） | 文件 → 更多保存 → 保存小棋盘截图 | `gui/Menu.java`；`gui/LizzieFrame.java` | 文件系统 / 系统剪贴板；桌面渲染 | P2 | 图片分享提升复盘结果传播，优先级低于棋谱和分析数据保存。 |
| 保存胜率图截图（`menu.saveWinrate`） | 文件 → 更多保存 → 保存胜率图截图 | `gui/Menu.java`；`gui/LizzieFrame.java` | 文件系统 / 系统剪贴板；桌面渲染 | P2 | 图片分享提升复盘结果传播，优先级低于棋谱和分析数据保存。 |
| 复制主棋盘到剪贴板（`menu.copyBoardScreen`） | 文件 → 复制主棋盘到剪贴板 | `gui/Menu.java`；`gui/LizzieFrame.java` | 文件系统 / 系统剪贴板；桌面渲染 | P2 | 图片分享提升复盘结果传播，优先级低于棋谱和分析数据保存。 |
| 复制小棋盘到剪贴板（`menu.copySubBoardScreen`） | 文件 → 复制小棋盘到剪贴板 | `gui/Menu.java`；`gui/LizzieFrame.java` | 文件系统 / 系统剪贴板；桌面渲染 | P2 | 图片分享提升复盘结果传播，优先级低于棋谱和分析数据保存。 |
| 快速启动外部程序（`tools.quick-start`） | 快速启动 → 已配置的程序；快速启动 → 设置 | `gui/Menu.java`、`gui/OtherPrograms.java`、`analysis/FastLink.java` | 用户配置的外部可执行程序及操作系统；是否联网由目标程序决定 | P2 | 外部程序启动是可选工具，可在基础复盘稳定后补充。 |
| 详细工具栏（中文界面）（`toolbar.detailed`） | 显示 → 工具栏 → 底部工具栏 → 详细工具栏（中文界面） | `gui/Menu.java`；`gui/BottomToolbar.java` | 依对应分析 / 自动播放功能；旧入口限中文 | 候选砍掉 | 保留对应分析能力，合并中文专属旧工具栏重复入口以降低维护成本。 |
| 显示快速启动（`config.display.quick-links`） | 设置 → 综合设置 → 棋盘与显示 → 显示快速启动 | `gui/ConfigDialog2.java`、`gui/Menu.java`、`analysis/FastLink.java` | 桌面 UI；实际启动需用户配置的外部程序 | P2 | 外部程序启动菜单的显示偏好随该可选工具提供。 |
| 离线功能搜索、分类浏览与设置定位（`extra.gui.function-search`） | 顶部搜索按钮；Ctrl+K，macOS Command+K | `gui/FunctionSearchDialog.java`、`search/FunctionCatalog.java`、`search/FunctionSearch.java` | 本地多语言词库；目标功能另有依赖 | P2 | 基本导航完成后，用离线搜索降低大型设置界面的查找成本。 |
| 工具栏自动换行、底栏更多动作及详细栏展开（`extra.gui.toolbar-overflow`） | 顶部工具栏；底部「更多」及详细栏按钮 | `gui/TopHeaderPanel.java`、`gui/BottomToolbar.java`、`gui/Menu.java` | 桌面窗口空间；无网络 | P2 | 保留窄窗口入口可达性，具体布局交由新版 UI 设计。 |

## 系统与发布

| 功能（中文名） | 用户入口 | Java 主要源码 | 外部依赖（引擎 / 网络 / 平台） | 建议层级 | 理由（一句话） |
| --- | --- | --- | --- | --- | --- |
| 强制退出（`menu.forceExit`） | 文件 → 强制退出 | `gui/Menu.java` | 操作系统进程 | 候选砍掉 | 强制退出是旧运行时的逃生入口，新版先依明确的关闭与恢复协议处理。 |
| 退出（`menu.exit`） | 文件 → 退出 | `gui/Menu.java` | 桌面运行环境 | MVP | 正常退出与版本信息是桌面发布的基本能力。 |
| 主棋盘/界面位置（`menu.mainBoardPos`） | 显示 → 主棋盘/界面位置 | `gui/Menu.java`；`gui/LizzieFrame.java` | 桌面窗口系统 | P3 | 自定义或多窗口布局应在单窗口复盘体验稳定后按需求补齐。 |
| 棋盘样式（`menu.boardStyleMenu`） | 显示 → 外观主题 → 棋盘样式 | `gui/Menu.java`；`gui/ConfigDialog2.java` | 旧 Swing 主题 / 本地资源 | 候选砍掉 | 多套历史主题与 Swing 外观不宜整体移植，保留一个一致且可读的新版默认主题。 |
| 重置界面位置（`menu.deletePersistFile`） | 显示 → 重置界面位置 | `gui/Menu.java`；`gui/LizzieFrame.java` | 桌面窗口系统 | P3 | 自定义或多窗口布局应在单窗口复盘体验稳定后按需求补齐。 |
| Apple 风格 UI（`menu.toggleAppleStyle`） | 显示 → 外观主题 → Apple 风格 UI | `gui/Menu.java`；`gui/ConfigDialog2.java` | 旧 Swing 主题 / 本地资源 | 候选砍掉 | 多套历史主题与 Swing 外观不宜整体移植，保留一个一致且可读的新版默认主题。 |
| 经典配色（原版 lizzieyzy）（`menu.toggleClassicColors`） | 显示 → 外观主题 → 经典配色（原版 lizzieyzy） | `gui/Menu.java`；`gui/ConfigDialog2.java` | 旧 Swing 主题 / 本地资源 | 候选砍掉 | 多套历史主题与 Swing 外观不宜整体移植，保留一个一致且可读的新版默认主题。 |
| 设置自定义棋盘背景...（`menu.setCustomBoard`） | 显示 → 外观主题 → 设置自定义棋盘背景... | `gui/Menu.java`；`gui/ConfigDialog2.java` | 本地图片 / 桌面绘制 | 候选砍掉 | 自定义图片与旧主题特效增加维护成本，待需求确认后再决定最小支持。 |
| 恢复默认棋盘背景（`menu.clearCustomBoard`） | 显示 → 外观主题 → 恢复默认棋盘背景 | `gui/Menu.java`；`gui/ConfigDialog2.java` | 本地图片 / 桌面绘制 | 候选砍掉 | 自定义图片与旧主题特效增加维护成本，待需求确认后再决定最小支持。 |
| 放大小棋盘（`menu.largeSubBoard`） | 显示 → 主界面设置 → 放大小棋盘 | `gui/Menu.java`；`gui/LizzieFrame.java` | 桌面窗口系统 | P3 | 自定义或多窗口布局应在单窗口复盘体验稳定后按需求补齐。 |
| 放大胜率图（`menu.largeWinrateGraph`） | 显示 → 主界面设置 → 放大胜率图 | `gui/Menu.java`；`gui/LizzieFrame.java` | 桌面窗口系统 | P3 | 自定义或多窗口布局应在单窗口复盘体验稳定后按需求补齐。 |
| 总在最前（`menu.alwaysOnTop`） | 显示 → 主界面设置 → 总在最前 | `gui/Menu.java`；`gui/LizzieFrame.java` | 桌面窗口系统 | P3 | 自定义或多窗口布局应在单窗口复盘体验稳定后按需求补齐。 |
| 显示清空棋盘提示对话框（`menu.showNewBoardConfirmDialog`） | 显示 → 主界面设置 → 显示清空棋盘提示对话框 | `gui/Menu.java` | 本地棋谱 / 文件系统 | MVP | 清空前确认保护用户棋谱；MVP 保留安全默认，关闭提示的开关可后置。 |
| 显示覆盖文件提示对话框（`menu.showReplaceFileConfirmDialog`） | 显示 → 主界面设置 → 显示覆盖文件提示对话框 | `gui/Menu.java` | 本地棋谱 / 文件系统 | MVP | 覆盖前确认保护用户棋谱；MVP 保留安全默认，关闭提示的开关可后置。 |
| 信息面板（`menu.informationPane`） | 显示 → 面板 → 信息面板 | `gui/Menu.java`；`gui/LizzieFrame.java` | 本地棋局 / 引擎状态 | MVP | 棋局与引擎状态可见性直接支撑 P1 可用性和故障定位。 |
| 状态面板（`menu.statusPanel`） | 显示 → 面板 → 状态面板 | `gui/Menu.java`；`gui/LizzieFrame.java` | 本地棋局 / 引擎状态 | MVP | 棋局与引擎状态可见性直接支撑 P1 可用性和故障定位。 |
| 可视化面板设置（`menu.visualizedPanelSettings`） | 显示 → 面板 → 可视化面板设置 | `gui/Menu.java`；`gui/LizzieFrame.java` | 桌面窗口系统 | P3 | 自定义或多窗口布局应在单窗口复盘体验稳定后按需求补齐。 |
| 浮动主棋盘（`menu.independentMainBoard`） | 显示 → 面板 → 浮动主棋盘 | `gui/Menu.java`；`gui/LizzieFrame.java` | 桌面窗口系统 | P3 | 自定义或多窗口布局应在单窗口复盘体验稳定后按需求补齐。 |
| 浮动主棋盘(保留原棋盘)（`menu.independentMainBoard2`） | 显示 → 面板 → 浮动主棋盘(保留原棋盘) | `gui/Menu.java`；`gui/LizzieFrame.java` | 桌面窗口系统 | P3 | 自定义或多窗口布局应在单窗口复盘体验稳定后按需求补齐。 |
| 浮动小棋盘（`menu.independentSubBoard`） | 显示 → 面板 → 浮动小棋盘 | `gui/Menu.java`；`gui/LizzieFrame.java` | 桌面窗口系统 | P3 | 自定义或多窗口布局应在单窗口复盘体验稳定后按需求补齐。 |
| 恢复默认面板尺寸（`menu.restoreDefaultPanelSizes`） | 显示 → 面板 → 恢复默认面板尺寸 | `gui/Menu.java`；`gui/LizzieFrame.java` | 桌面窗口系统 | P3 | 自定义或多窗口布局应在单窗口复盘体验稳定后按需求补齐。 |
| 布局模式（`menu.layoutModeMenu`） | 显示 → 布局模式 | `gui/Menu.java`；`gui/LizzieFrame.java` | 桌面窗口系统 | P3 | 自定义或多窗口布局应在单窗口复盘体验稳定后按需求补齐。 |
| 浮动棋盘模式（`menu.independentBoardMode`） | 显示 → 浮动棋盘模式 | `gui/Menu.java`；`gui/LizzieFrame.java` | 桌面窗口系统 | P3 | 自定义或多窗口布局应在单窗口复盘体验稳定后按需求补齐。 |
| 顶部工具栏（`menu.topToolBar`） | 显示 → 工具栏 → 顶部工具栏 | `gui/Menu.java` | 桌面 UI | MVP | 承载分析、翻手与文件操作的基本工具栏属于 P1 主线。 |
| 自动换行(仅独立显示时)（`menu.autoWrap`） | 显示 → 工具栏 → 顶部工具栏 → 自动换行(仅独立显示时) | `gui/Menu.java`；`gui/LizzieFrame.java` | 桌面窗口系统 | P3 | 自定义或多窗口布局应在单窗口复盘体验稳定后按需求补齐。 |
| 底部工具栏（`menu.bottomToolBar`） | 显示 → 工具栏 → 底部工具栏 | `gui/Menu.java`；`gui/BottomToolbar.java` | 桌面 UI | MVP | 承载分析、翻手与文件操作的基本工具栏属于 P1 主线。 |
| 自定义顶部工具栏内容（`menu.customTopToolBar`） | 显示 → 工具栏 → 顶部工具栏 → 自定义内容 | `gui/Menu.java`；`gui/LizzieFrame.java` | 桌面窗口系统 | P3 | 自定义或多窗口布局应在单窗口复盘体验稳定后按需求补齐。 |
| 自定义底部工具栏内容（`menu.customToolbarItem`） | 显示 → 工具栏 → 底部工具栏 → 自定义内容 | `gui/Menu.java`；`gui/LizzieFrame.java` | 桌面窗口系统 | P3 | 自定义或多窗口布局应在单窗口复盘体验稳定后按需求补齐。 |
| 诊断与日志（`menu.diagnosticsAndLogs`） | 帮助 → 诊断与日志 | `gui/Menu.java`；`gui/DiagnosticsDialog.java` | 本地日志 / 文件系统 | MVP | 诊断包和可控日志是 ROADMAP P2 的故障支持要求。 |
| 停止完整日志（`menu.stopFullTrace`） | 帮助 → 停止完整日志 | `gui/Menu.java`；`gui/DiagnosticsDialog.java` | 本地日志；需已开启完整日志 | MVP | 诊断包和可控日志是 ROADMAP P2 的故障支持要求。 |
| 关于（`menu.about`） | 帮助 → 关于 | `gui/Menu.java` | 桌面运行环境 | MVP | 正常退出与版本信息是桌面发布的基本能力。 |
| 检查更新（`menu.checkUpdate`） | 帮助 → 检查更新 | `gui/Menu.java` | 网络；发布服务 / 平台安装权限 | MVP | 自动更新与签名验证是 ROADMAP P2 发布闭环的一部分。 |
| 清除所有个人数据（实际清除四类历史）（`menu.clearUserData`） | 帮助 → 清除所有个人数据 | `gui/Menu.java` | 本地配置中的野狐搜索、最近文件、批量分析及分享历史 | P2 | Java 此入口只清除四类历史记录，新版应明确清除范围而不沿用过宽名称。 |
| 初始化设置（`menu.initSettings`） | 设置 → 初始化设置 | `gui/Menu.java` | 本地配置与数据文件 | P2 | 数据清除与恢复默认需明确范围，可在独立配置目录建立后补齐。 |
| 主题（`menu.theme`） | 设置 → 主题 | `gui/Menu.java`；`gui/ConfigDialog2.java` | 旧 Swing 主题 / 本地资源 | 候选砍掉 | 多套历史主题与 Swing 外观不宜整体移植，保留一个一致且可读的新版默认主题。 |
| 语言（`menu.language`） | 设置 → 语言 | `gui/Menu.java` | 本地翻译资源 | MVP | P2 明确要求中英文全界面；其余 Java 语言不自动进入 MVP。 |
| 界面字体大小（`menu.frameFontSize`） | 设置 → 界面字体大小 | `gui/Menu.java`；`gui/ConfigDialog2.java` | 系统字体 / DPI | P2 | 字体可读性设置有助于不同屏幕，默认可用字号先随 MVP 交付。 |
| 界面外观（`menu.frameLooks`） | 设置 → 界面外观 | `gui/Menu.java`；`gui/ConfigDialog2.java` | 旧 Swing 主题 / 本地资源 | 候选砍掉 | 多套历史主题与 Swing 外观不宜整体移植，保留一个一致且可读的新版默认主题。 |
| 播放落子声（`menu.playSound`） | 设置 → 播放落子声 | `gui/Menu.java` | 系统音频输出 | P2 | 落子声为可选反馈，基础复盘流程无需音频即可完成。 |
| 自定义布局 1（`menu.custom-layout-1`） | 显示 → 自定义布局 1 | `gui/Menu.java`；`gui/LizzieFrame.java` | 桌面窗口系统 | P3 | 自定义或多窗口布局应在单窗口复盘体验稳定后按需求补齐。 |
| 自定义布局 2（`menu.custom-layout-2`） | 显示 → 自定义布局 2 | `gui/Menu.java`；`gui/LizzieFrame.java` | 桌面窗口系统 | P3 | 自定义或多窗口布局应在单窗口复盘体验稳定后按需求补齐。 |
| 窗口总在最前（`config.display.always-on-top`） | 设置 → 综合设置 → 棋盘与显示 → 窗口总在最前 | `gui/ConfigDialog2.java`；`gui/LizzieFrame.java` | 桌面窗口系统 | P3 | 自定义或多窗口布局应在单窗口复盘体验稳定后按需求补齐。 |
| 显示状态面板（`config.display.status`） | 设置 → 综合设置 → 棋盘与显示 → 显示状态面板 | `gui/ConfigDialog2.java`；`gui/LizzieFrame.java` | 本地棋局 / 引擎状态 | MVP | 棋局与引擎状态可见性直接支撑 P1 可用性和故障定位。 |
| 当前主题（`config.theme.current`） | 设置 → 综合设置 → 主题外观 → 当前主题 | `gui/ConfigDialog2.java` | 旧 Swing 主题 / 本地资源 | 候选砍掉 | 多套历史主题与 Swing 外观不宜整体移植，保留一个一致且可读的新版默认主题。 |
| 棋子阴影大小（`config.theme.shadow`） | 设置 → 综合设置 → 主题外观 → 棋子阴影大小 | `gui/ConfigDialog2.java` | 本地图片 / 桌面绘制 | 候选砍掉 | 自定义图片与旧主题特效增加维护成本，待需求确认后再决定最小支持。 |
| 计算量及其他字体（`config.theme.info-font`） | 设置 → 综合设置 → 主题外观 → 计算量及其他字体 | `gui/ConfigDialog2.java` | 系统字体 / DPI | P2 | 字体可读性设置有助于不同屏幕，默认可用字号先随 MVP 交付。 |
| UI 字体（`config.theme.ui-font`） | 设置 → 综合设置 → 主题外观 → UI 字体 | `gui/ConfigDialog2.java` | 系统字体 / DPI | P2 | 字体可读性设置有助于不同屏幕，默认可用字号先随 MVP 交付。 |
| 胜率目数字体（`config.theme.winrate-font`） | 设置 → 综合设置 → 主题外观 → 胜率目数字体 | `gui/ConfigDialog2.java` | 系统字体 / DPI | P2 | 字体可读性设置有助于不同屏幕，默认可用字号先随 MVP 交付。 |
| 背景图片（`config.theme.background-image`） | 设置 → 综合设置 → 主题外观 → 背景图片 | `gui/ConfigDialog2.java` | 本地图片 / 桌面绘制 | 候选砍掉 | 自定义图片与旧主题特效增加维护成本，待需求确认后再决定最小支持。 |
| 棋盘图片（`config.theme.board-image`） | 设置 → 综合设置 → 主题外观 → 棋盘图片 | `gui/ConfigDialog2.java` | 本地图片 / 桌面绘制 | 候选砍掉 | 自定义图片与旧主题特效增加维护成本，待需求确认后再决定最小支持。 |
| 黑子图片（`config.theme.black-stone-image`） | 设置 → 综合设置 → 主题外观 → 黑子图片 | `gui/ConfigDialog2.java` | 本地图片 / 桌面绘制 | 候选砍掉 | 自定义图片与旧主题特效增加维护成本，待需求确认后再决定最小支持。 |
| 白子图片（`config.theme.white-stone-image`） | 设置 → 综合设置 → 主题外观 → 白子图片 | `gui/ConfigDialog2.java` | 本地图片 / 桌面绘制 | 候选砍掉 | 自定义图片与旧主题特效增加维护成本，待需求确认后再决定最小支持。 |
| 面板背景模糊程度（`config.theme.background-blur`） | 设置 → 综合设置 → 主题外观 → 面板背景模糊程度 | `gui/ConfigDialog2.java` | 本地图片 / 桌面绘制 | 候选砍掉 | 自定义图片与旧主题特效增加维护成本，待需求确认后再决定最小支持。 |
| 评论字体大小（`config.theme.comment-font-size`） | 设置 → 综合设置 → 主题外观 → 评论字体大小 | `gui/ConfigDialog2.java` | 系统字体 / DPI | P2 | 字体可读性设置有助于不同屏幕，默认可用字号先随 MVP 交付。 |
| 日志模块开关、限时全量跟踪与诊断包导出（`extra.gui.diagnostics-controls`） | 帮助 → 诊断与日志 | `gui/DiagnosticsDialog.java` | 本地日志；readboard 辅助诊断需相应平台工具 | MVP | P2 明确要求诊断包；同步专用日志部分随 P3 同步功能再补。 |
| 旧同步诊断摘要窗口（`extra.gui.legacy-sync-diagnostics`） | 未发现生产代码创建入口；仅保留对话框类 | `gui/SyncDiagnosticsDialog.java` | readboard / 弈客诊断状态；本地导出 | 候选砍掉 | 不将未接入的旧窗口视为已可用入口，优先评估与统一诊断合并。 |
| Windows 安装器及便携版、macOS DMG、Linux 分发包 `extra.release.platforms` | 官网 / GitHub Releases 下载并安装或解压 | `Lizzie.java`、`logging/WorkDirectoryResolver.java`；`scripts/package_windows_exe.sh`、`scripts/package_macos_dmg.sh`、`scripts/package_release.sh` | Windows / macOS / Linux；Java 旧包携带 Java runtime | MVP | P1 要三平台可打包，P2 公开预览版仍需清楚的安装入口但不照搬 JVM 包体。 |
| macOS 发布签名与公证 `extra.release.signing` | 下载 DMG 后从 Finder 安装启动 | `scripts/sign_macos_release.sh`、`scripts/package_macos_dmg.sh`（发布层，无对应 Java 业务类） | macOS；官方流程需 Apple 签名及公证服务，本地无凭据构建可跳过 | MVP | ROADMAP P2 明确要求签名，正常安装链路需可信分发。 |
| 签名清单、断点续传与下载哈希校验 `extra.update.integrity` | 帮助 → 检查更新 → 下载、暂停、继续 | `update/SignedUpdateEnvelope.java`、`update/ResumableDownloader.java` | 更新源网络；Ed25519 与 SHA-256 | MVP | 自动更新属于 P2 必需能力，执行下载内容前须校验其来源和完整性。 |
| Windows 核心更新、备份及失败回退 `extra.update.windows` | 检查更新 → 下载后安装；亦可手动覆盖 core-update 包 | `update/WindowsUpdateService.java`、`update/WindowsUpdateApplier.java` | Windows；安装目录写权限；更新源网络 | MVP | P2 自动更新必须保留棋谱配置并具有失败恢复路径。 |
| macOS / Linux 下载整包并打开安装位置 `extra.update.packages` | 检查更新 → 下载后打开 DMG / 所在文件夹 | `update/PlatformUpdateService.java`、`update/PackageUpdateDialog.java` | macOS / Linux 桌面集成；更新源网络 | MVP | 保留三平台更新入口，同时明确 Java 在这两平台由用户完成整包替换。 |
| 稳定 / 测试更新通道与官网 / GitHub 更新源 `extra.update.channels` | 检查更新窗口选择通道、来源 | `update/CheckUpdateDialog.java`、`update/UpdateChannel.java`、`update/UpdateSource.java` | 官网或 GitHub 网络 | P2 | 稳定通道属于基本更新，额外通道与源选择可在 P2 后补。 |
| 启动自动检查及自动下载偏好 `extra.update.preferences` | 检查更新窗口的自动检查 / 自动下载选项 | `update/UpdatePreferences.java`、`update/UpdateCheckCoordinator.java` | 更新源网络；本地偏好保存 | P2 | 自动更新的基础流程先保留，后台调度偏好可同阶段补充。 |
| 便携数据目录与 Java 旧配置兼容 `extra.system.user-data` | Windows 便携启动；移动目录或覆盖升级后继续使用配置 | `Config.java`、`logging/WorkDirectoryResolver.java` | 本地文件；Windows 便携标记 | MVP | P2 配置迁移需要明确旧数据位置，新版应独立存储并显式导入而非覆盖 Java 数据。 |

## 与新版路线图的对应

以本仓库的 [ROADMAP](../ROADMAP.md) 和 [ARCHITECTURE](../ARCHITECTURE.md) 为新版边界；Java 的 README / ROADMAP 只补充旧版入口及产品语境。Java 文档中的「接下来完善」不作为已交付能力计入。

| 新版范围 | 本清单对应 | 定级及边界 |
| --- | --- | --- |
| P1 打开 SGF、棋盘、实时分析、胜率图、翻手、最小引擎设置、三平台包 | `files.open`、`analysis.toggle`、`review.previous` / `review.next`、`engine.select`、`extra.release.platforms` 等 | MVP；保留用户主线，协议采用新版 analysis JSON。 |
| P2 整盘分析与失误列表 | `whole-game-deep-analysis`、`whole-game-lightning-overview`、`analysis.bad-moves` | MVP；旧版多种入口可以合并，双引擎及旧逐手调度机制不必照搬。 |
| P2 SGF 编辑与变化树 | `files.save`、`menu.setAsMain`、`menu.variationPane`、棋盘标记与注释相关行 | MVP；细化外观和自动播放偏好可后补。 |
| P2 SQLite 分析缓存 | Java 的棋谱分析数据参考 `extra.files.lz`；没有把 Java 的内存或 SGF 存储称为 SQLite 缓存 | 新版规划必需；实现方式由新版任务卡决定，不伪列 Java 已有功能。 |
| P2 KataGo 分发、引擎向导与权重下载 | `engine.setup`、`weights.download`、`extra.engine.distribution` | MVP；共享目录、哈希和运行时规则，多种实验完整包仍是候选降级项。 |
| P2 野狐昵称抓谱 | `menu.foxKifu` | MVP；野狐在线实战、readboard 和弈客不随抓谱前移。 |
| P2 LZ 读写与 GIB 导入 | `extra.files.lz`、`extra.files.gib`、`extra.files.encoding` | MVP；双引擎扩展的完整交互后置，未知 SGF 属性仍须保留。 |
| P2 中英文全界面 | `menu.language` | MVP 仅要求中文及英文；Java 其余四种语言不因共用菜单而自动成为 MVP 要求。 |
| P2 设置与 Java 配置导入 | `settings.general`、`extra.system.user-data` | MVP 保留基本设置；Java→新版显式导入是新增需求，旧版目录迁移仅作来源参考。 |
| P2 诊断包、自动更新与签名 | `menu.diagnosticsAndLogs`、`menu.checkUpdate`、`extra.update.*`、`extra.release.signing` | MVP；后台偏好和额外更新通道可 P2 后补，macOS / Linux 的 Java 更新流程不能误称为自动替换应用。 |
| P3 GTP 对局、HumanSL、readboard、远程算力、弈客、棋力评估、AI 解说、死活题、网页棋盘 | 相应「对局与训练」「在线与同步」「工具」功能行 | P3；旧版现成不意味着新版提前实现。 |
| 候选砍掉：Leela Zero 专用适配、历史主题、内嵌浏览器、低下载量 GPU 变体 | 相应引擎、主题、浏览器及 `extra.engine.experimental-packages` 行 | 仅供维护者取舍；下载次数不能单独证明某硬件用户不需要支持。 |

「建议层级」是供审阅的产品建议，既不替代下一阶段任务卡，也不表示新版已有实现。保留某设置的基本效果不必保留旧版的同名开关；尤其所有分析值仍须遵守新版固定黑方视角。

## 顶层入口索引

`Menu.java` 构造器向菜单栏加入 12 个菜单和 2 个动作按钮，共 14 个功能入口；两个水平间隔不计数。
菜单标题按中文词库解码，动态引擎标题按角色列出，条件隐藏的跑谱贡献仍计入。

| 顶层入口 | 构造变量及位置 | 对应功能 ID |
| --- | --- | --- |
| 文件 | `fileMenu`，`gui/Menu.java:255` | `files.new`、`files.open`、`files.save`、`files.recent` |
| 显示 | `viewMenu`，`gui/Menu.java:552` | `menu.winrateGraph`、`menu.variationPane`、`menu.topToolBar` |
| 棋局 | `gameMenu`，`gui/Menu.java:2964` | `menu.newGenmoveGame`、`menu.newEngineGame`、`menu.newHumanSlGame` |
| 分析 | `analyzeMenu`，`gui/Menu.java:3353` | `analysis.toggle`、`whole-game-deep-analysis`、`whole-game-lightning-overview` |
| 编辑 | `editMenu`，`gui/Menu.java:3617` | `menu.addBlack`、`menu.delete`、`menu.undoDelete`、`game.komi` |
| 同步 | `live`，`gui/Menu.java:3975` | `menu.yikeLive`、`menu.foxKifu`、`extra.sync.fox-direct`、`sync.board`、`menu.webBoardToggle` |
| 帮助 | `helpMenu`，`gui/Menu.java:5005` | `menu.diagnosticsAndLogs`、`menu.about`、`menu.checkUpdate` |
| 快速启动 | `quickLinks`，`gui/Menu.java:5096` | `tools.quick-start` |
| 设置 | `settings`，`gui/Menu.java:5103` | `engine.configuration`、`engine.setup`、`settings.general` |
| 跑谱贡献（可隐藏） | `contributeMenu`，`gui/Menu.java:5420` | `menu.kataGoDistributedTraining`、`menu.kataGoTrainingSettings` |
| 主引擎（动态名称，初始为未加载引擎） | `engineMenu`，`gui/Menu.java:5469` | `engine.select`、`menu.restartCurrentEngine`、`menu.shutdownAllEngine` |
| 副引擎（动态名称，初始为未加载引擎） | `engineMenu2`，`gui/Menu.java:5478` | `engine.select-secondary`、`menu.restartCurrentEngine2` |
| AI 陪练（按钮） | `aiCoachButton`，`gui/Menu.java:5511` | `game.ai-coach`、`extra.gui.coach-actions` |
| AI 解说（按钮） | `aiCommentaryButton`，`gui/Menu.java:5513` | `analysis.commentary` |

## 窗口与面板索引

`gui/` 直属目录中 `*Dialog.java`、`*Frame.java`、`*Panel.java` 共 36 个文件。
下面按实际职责映射；文件名以 Frame 结尾不必然表示独立窗口（例如 `WinrateGraphFrame` 是图表绘制辅助类）。

| Java GUI 文件 | 对应功能 ID / 职责 |
| --- | --- |
| `AnalysisFrame.java` | `menu.SuggestionList`、`extra.gui.candidate-window` |
| `B11SpeedNoticePanel.java` | `extra.gui.engine-benchmark`：测速旁的 B11 速度 / 棋力说明 |
| `BlunderListPanel.java` | `extra.gui.problem-sidebar` |
| `BrowserFrame.java` | `extra.gui.embedded-browser` |
| `CaptureTsumeGoFrame.java` | `menu.captureTsumeGo`、`extra.gui.tsumego-options` |
| `CoachPanel.java` | `game.ai-coach`、`extra.gui.coach-actions` |
| `DiagnosticsDialog.java` | `menu.diagnosticsAndLogs`、`extra.gui.diagnostics-controls` |
| `EngineGameParametersDialog.java` | `extra.gui.engine-game-parameters` |
| `FunctionSearchDialog.java` | `extra.gui.function-search` |
| `GameInfoDialog.java` | `game.komi`：对局者、贴目、让子信息 |
| `GtpEngineConfigDialog.java` | `extra.gui.gtp-strength-config` |
| `HumanSlStrengthDialog.java` | `extra.gui.strength-models`：显式准备进度与可取消模型下载 |
| `KataGoAutoSetupDialog.java` | `engine.setup`、`weights.download`、`engine.acceleration`、`extra.gui.engine-repair`、`extra.gui.local-weights`、`extra.gui.quick-model`、`extra.gui.humansl-assets`、`extra.gui.engine-benchmark`、`extra.gui.experimental-backends` |
| `LizzieFrame.java` | `files.open`、`analysis.toggle`、`extra.gui.problem-sidebar` 等主窗口操作及导航 |
| `MatchRulesDetailsDialog.java` | `extra.gui.match-rules-details` |
| `MeasuredTuningDialog.java` | `extra.gui.measured-tuning` |
| `MoveListFrame.java` | `analysis.bad-moves`、`extra.gui.review-statistics`、`extra.gui.review-table-export` |
| `MovenumberDialog.java` | `menu.moveMenu`：最近显示手数设置 |
| `NewAnaGameDialog.java` | `menu.newAnalyzeModeGame`、`extra.gui.human-game-options` |
| `NewEngineGameDialog.java` | `menu.newEngineGame`、`extra.gui.engine-match-options` |
| `NewGameDialog.java` | `menu.newGenmoveGame`、`extra.gui.human-game-options` |
| `NewHumanSlGameDialog.java` | `menu.newHumanSlGame`、`extra.gui.humansl-assets`、`extra.gui.coach-actions` |
| `OnlineDialog.java` | `files.url`、`extra.gui.yike-live-list`：在线棋谱加载与刷新 |
| `RemoteComputeDialog.java` | `engine.remote`、`extra.gui.remote-login`、`extra.gui.remote-custom` |
| `SidebarHeaderPanel.java` | `extra.gui.problem-sidebar`：评论 / 问题手标签与分析进度 |
| `SidebarPanel.java` | `extra.gui.problem-sidebar`、`extra.gui.comment-edit` |
| `StartAnaDialog.java` | `auto-analysis`、`batch-analysis`、`extra.gui.auto-analysis-options` |
| `StrengthDialog.java` | `menu.playerStrengthEstimate`、`extra.gui.strength-models` |
| `SyncDiagnosticsDialog.java` | `extra.gui.legacy-sync-diagnostics`：旧类保留，生产入口未确认 |
| `TopHeaderPanel.java` | `extra.gui.toolbar-overflow`：顶部控件分组与换行 |
| `TsumeGoFrame.java` | `game.tsumego`、`extra.gui.tsumego-options` |
| `WholeGameAnalysisDialog.java` | `whole-game-deep-analysis`、`extra.gui.whole-game-controls` |
| `WinrateGraphFrame.java` | `menu.winrateGraph`：图例与刻度绘制辅助 |
| `YikeLiveDialog.java` | `menu.yikeLive`、`extra.gui.yike-live-list` |
| `ZhiziAccountDetailsDialog.java` | `extra.gui.remote-account` |
| `ZhiziVipPurchaseDialog.java` | `extra.gui.remote-payment` |

底部工具栏普通按钮、直播弹出菜单、自动分析 / 自动播放 / 引擎比赛详细栏分别落在
`review.*`、`board.*`、`menu.*`、`legacy.*` 目录项及上述补充行；「更多」只是窄窗口下同一动作的入口。
Java README 和 ROADMAP 用于交叉核对首启、权重、抓谱、平台包与远程能力；其中未来愿望未当作已实现功能。

## 下载数据

采集时间：**2026-10-29 13:22–13:24 UTC**。使用无需登录的 GitHub REST API，筛选 `draft=false` 且 `prerelease=false`；最近三条正式发布记录的 `published_at` 由新到旧如下。未按标签名称猜测正式版，也未用预发布版补数。

通过公开列表接口每页 5 条读取 [第 1 页](https://api.github.com/repos/wimi321/lizzieyzy-next/releases?per_page=5&page=1) 至 [第 8 页](https://api.github.com/repos/wimi321/lizzieyzy-next/releases?per_page=5&page=8)，共 40 条记录；筛选得到 10 条正式版后，按 `published_at` 降序取最近 3 条，再逐版读取资产。下载数取自对应资产的 `download_count`，未下载大型安装包。

| 正式版 | published_at（UTC） | 资产数据 |
| --- | --- | --- |
| [next-2026-10-24.2](https://github.com/wimi321/lizzieyzy-next/releases/tag/next-2026-10-24.2) | 2026-10-24T05:14:03Z | [公开 API](https://api.github.com/repos/wimi321/lizzieyzy-next/releases/422465580) |
| [next-2026-10-17.1](https://github.com/wimi321/lizzieyzy-next/releases/tag/next-2026-10-17.1) | 2026-10-17T17:34:48Z | [公开 API](https://api.github.com/repos/wimi321/lizzieyzy-next/releases/415895310) |
| [next-2026-09-26.2](https://github.com/wimi321/lizzieyzy-next/releases/tag/next-2026-09-26.2) | 2026-09-26T17:28:32Z | [公开 API](https://api.github.com/repos/wimi321/lizzieyzy-next/releases/397289542) |

### 统计口径

“安装包”按可首次安装或直接运行的完整分发包统计，含 Windows 安装器与便携 ZIP、macOS DMG、Linux ZIP，以及在正式 release 中明确标为实验的完整包。平台、后端、是否含引擎分别保留，Windows 同变体的安装器与便携包在汇总表相加。依据为各 release 的下载说明及 Java 仓库 `README.md`、`docs/PACKAGES.md`。

每版有 43 个资产：19 个单文件完整包、2 个 TensorRT 分卷、1 个仅供升级的 `core-update.zip`、15 个 KataGo 引擎组件，以及 6 个说明/校验/更新元数据文件。TensorRT 两卷合成一套包，逐卷列数但不计入单文件完整包总计；不能把两卷下载量相加称为安装套数。升级包另列；`katago-source-*.zip`、分卷 README/manifest/SHA-256、下载目录和更新 manifest/envelope 不计为安装包。

### 每个完整安装包的下载数

#### next-2026-10-24.2

| 资产完整文件名 | 平台 / 变体 | 下载数 |
| --- | --- | ---: |
| `2026-10-24-linux64.nvidia.zip` | Linux x64 / NVIDIA CUDA | 4 |
| `2026-10-24-linux64.opencl.zip` | Linux x64 / OpenCL | 9 |
| `2026-10-24-linux64.with-katago.zip` | Linux x64 / CPU | 5 |
| `2026-10-24-mac-apple-silicon.with-katago.dmg` | macOS Apple Silicon / 整合包 | 21 |
| `2026-10-24-mac-intel.with-katago.dmg` | macOS Intel / 整合包 | 9 |
| `2026-10-24-windows64.experimental.directml.portable.zip` | Windows x64 / DirectML（实验） | 2 |
| `2026-10-24-windows64.experimental.openvino.portable.zip` | Windows x64 / OpenVINO（实验） | 5 |
| `2026-10-24-windows64.experimental.rocm.gfx103x.portable.zip` | Windows x64 / ROCm gfx103x（实验） | 15 |
| `2026-10-24-windows64.experimental.rocm.gfx110x.portable.zip` | Windows x64 / ROCm gfx110x（实验） | 4 |
| `2026-10-24-windows64.experimental.rocm.gfx1151.portable.zip` | Windows x64 / ROCm gfx1151（实验） | 2 |
| `2026-10-24-windows64.experimental.rocm.gfx120x.portable.zip` | Windows x64 / ROCm gfx120x（实验） | 20 |
| `2026-10-24-windows64.nvidia.installer.exe` | Windows x64 / NVIDIA CUDA | 9 |
| `2026-10-24-windows64.nvidia.portable.zip` | Windows x64 / NVIDIA CUDA | 128 |
| `2026-10-24-windows64.opencl.installer.exe` | Windows x64 / OpenCL | 5 |
| `2026-10-24-windows64.opencl.portable.zip` | Windows x64 / OpenCL | 38 |
| `2026-10-24-windows64.with-katago.installer.exe` | Windows x64 / CPU | 2 |
| `2026-10-24-windows64.with-katago.portable.zip` | Windows x64 / CPU | 28 |
| `2026-10-24-windows64.without.engine.installer.exe` | Windows x64 / 不含引擎 | 8 |
| `2026-10-24-windows64.without.engine.portable.zip` | Windows x64 / 不含引擎 | 45 |
| **19 个单文件完整包合计** | — | **359** |

#### next-2026-10-17.1

| 资产完整文件名 | 平台 / 变体 | 下载数 |
| --- | --- | ---: |
| `2026-10-17-linux64.nvidia.zip` | Linux x64 / NVIDIA CUDA | 8 |
| `2026-10-17-linux64.opencl.zip` | Linux x64 / OpenCL | 11 |
| `2026-10-17-linux64.with-katago.zip` | Linux x64 / CPU | 11 |
| `2026-10-17-mac-apple-silicon.with-katago.dmg` | macOS Apple Silicon / 整合包 | 33 |
| `2026-10-17-mac-intel.with-katago.dmg` | macOS Intel / 整合包 | 8 |
| `2026-10-17-windows64.experimental.directml.portable.zip` | Windows x64 / DirectML（实验） | 3 |
| `2026-10-17-windows64.experimental.openvino.portable.zip` | Windows x64 / OpenVINO（实验） | 5 |
| `2026-10-17-windows64.experimental.rocm.gfx103x.portable.zip` | Windows x64 / ROCm gfx103x（实验） | 3 |
| `2026-10-17-windows64.experimental.rocm.gfx110x.portable.zip` | Windows x64 / ROCm gfx110x（实验） | 3 |
| `2026-10-17-windows64.experimental.rocm.gfx1151.portable.zip` | Windows x64 / ROCm gfx1151（实验） | 3 |
| `2026-10-17-windows64.experimental.rocm.gfx120x.portable.zip` | Windows x64 / ROCm gfx120x（实验） | 23 |
| `2026-10-17-windows64.nvidia.installer.exe` | Windows x64 / NVIDIA CUDA | 11 |
| `2026-10-17-windows64.nvidia.portable.zip` | Windows x64 / NVIDIA CUDA | 156 |
| `2026-10-17-windows64.opencl.installer.exe` | Windows x64 / OpenCL | 7 |
| `2026-10-17-windows64.opencl.portable.zip` | Windows x64 / OpenCL | 50 |
| `2026-10-17-windows64.with-katago.installer.exe` | Windows x64 / CPU | 14 |
| `2026-10-17-windows64.with-katago.portable.zip` | Windows x64 / CPU | 24 |
| `2026-10-17-windows64.without.engine.installer.exe` | Windows x64 / 不含引擎 | 3 |
| `2026-10-17-windows64.without.engine.portable.zip` | Windows x64 / 不含引擎 | 30 |
| **19 个单文件完整包合计** | — | **406** |

#### next-2026-09-26.2

| 资产完整文件名 | 平台 / 变体 | 下载数 |
| --- | --- | ---: |
| `2026-09-26-linux64.nvidia.zip` | Linux x64 / NVIDIA CUDA | 33 |
| `2026-09-26-linux64.opencl.zip` | Linux x64 / OpenCL | 13 |
| `2026-09-26-linux64.with-katago.zip` | Linux x64 / CPU | 22 |
| `2026-09-26-mac-apple-silicon.with-katago.dmg` | macOS Apple Silicon / 整合包 | 44 |
| `2026-09-26-mac-intel.with-katago.dmg` | macOS Intel / 整合包 | 17 |
| `2026-09-26-windows64.experimental.directml.portable.zip` | Windows x64 / DirectML（实验） | 24 |
| `2026-09-26-windows64.experimental.openvino.portable.zip` | Windows x64 / OpenVINO（实验） | 15 |
| `2026-09-26-windows64.experimental.rocm.gfx103x.portable.zip` | Windows x64 / ROCm gfx103x（实验） | 17 |
| `2026-09-26-windows64.experimental.rocm.gfx110x.portable.zip` | Windows x64 / ROCm gfx110x（实验） | 38 |
| `2026-09-26-windows64.experimental.rocm.gfx1151.portable.zip` | Windows x64 / ROCm gfx1151（实验） | 27 |
| `2026-09-26-windows64.experimental.rocm.gfx120x.portable.zip` | Windows x64 / ROCm gfx120x（实验） | 30 |
| `2026-09-26-windows64.nvidia.installer.exe` | Windows x64 / NVIDIA CUDA | 37 |
| `2026-09-26-windows64.nvidia.portable.zip` | Windows x64 / NVIDIA CUDA | 202 |
| `2026-09-26-windows64.opencl.installer.exe` | Windows x64 / OpenCL | 22 |
| `2026-09-26-windows64.opencl.portable.zip` | Windows x64 / OpenCL | 88 |
| `2026-09-26-windows64.with-katago.installer.exe` | Windows x64 / CPU | 29 |
| `2026-09-26-windows64.with-katago.portable.zip` | Windows x64 / CPU | 79 |
| `2026-09-26-windows64.without.engine.installer.exe` | Windows x64 / 不含引擎 | 11 |
| `2026-09-26-windows64.without.engine.portable.zip` | Windows x64 / 不含引擎 | 65 |
| **19 个单文件完整包合计** | — | **813** |

### 按变体汇总（完整单文件包）

| 平台 / 变体 | 10-24.2 | 10-17.1 | 09-26.2 | 三版合计 |
| --- | ---: | ---: | ---: | ---: |
| Windows x64 / NVIDIA CUDA | 137 | 167 | 239 | 543 |
| Windows x64 / OpenCL | 43 | 57 | 110 | 210 |
| Windows x64 / CPU | 30 | 38 | 108 | 176 |
| Windows x64 / 不含引擎 | 53 | 33 | 76 | 162 |
| Windows x64 / DirectML（实验） | 2 | 3 | 24 | 29 |
| Windows x64 / OpenVINO（实验） | 5 | 5 | 15 | 25 |
| Windows x64 / ROCm gfx103x（实验） | 15 | 3 | 17 | 35 |
| Windows x64 / ROCm gfx110x（实验） | 4 | 3 | 38 | 45 |
| Windows x64 / ROCm gfx1151（实验） | 2 | 3 | 27 | 32 |
| Windows x64 / ROCm gfx120x（实验） | 20 | 23 | 30 | 73 |
| macOS Apple Silicon / 整合包 | 21 | 33 | 44 | 98 |
| macOS Intel / 整合包 | 9 | 8 | 17 | 34 |
| Linux x64 / CPU | 5 | 11 | 22 | 38 |
| Linux x64 / OpenCL | 9 | 11 | 13 | 33 |
| Linux x64 / NVIDIA CUDA | 4 | 8 | 33 | 45 |
| **合计** | **359** | **406** | **813** | **1578** |

### TensorRT 分卷及仅升级包（不混入上表）

| 资产完整文件名 | 类型 | 下载数 |
| --- | --- | ---: |
| `2026-10-24-windows64.core-update.zip` | Windows 仅升级包 | 47 |
| `2026-10-24-windows64.nvidia.tensorrt.portable.7z.001` | Windows TensorRT 分卷（两卷均需下载） | 36 |
| `2026-10-24-windows64.nvidia.tensorrt.portable.7z.002` | Windows TensorRT 分卷（两卷均需下载） | 21 |
| `2026-10-17-windows64.core-update.zip` | Windows 仅升级包 | 57 |
| `2026-10-17-windows64.nvidia.tensorrt.portable.7z.001` | Windows TensorRT 分卷（两卷均需下载） | 40 |
| `2026-10-17-windows64.nvidia.tensorrt.portable.7z.002` | Windows TensorRT 分卷（两卷均需下载） | 23 |
| `2026-09-26-windows64.core-update.zip` | Windows 仅升级包 | 111 |
| `2026-09-26-windows64.nvidia.tensorrt.portable.7z.001` | Windows TensorRT 分卷（两卷均需下载） | 45 |
| `2026-09-26-windows64.nvidia.tensorrt.portable.7z.002` | Windows TensorRT 分卷（两卷均需下载） | 23 |

| 独立统计项 | 10-24.2 | 10-17.1 | 09-26.2 | 三版合计 |
| --- | ---: | ---: | ---: | ---: |
| TensorRT 第 1 卷请求 | 36 | 40 | 45 | 121 |
| TensorRT 第 2 卷请求 | 21 | 23 | 23 | 67 |
| Windows 仅升级包 | 47 | 57 | 111 | 215 |

### 可用于定级的结论与限制

在这三个正式版的 GitHub 单文件完整包中，Windows NVIDIA CUDA 合计 **543** 次最多；Windows OpenCL **210** 次、CPU **176** 次、不含引擎 **162** 次，也有明确下载需求。实验包的三版合计按硬件变体为 **25–73** 次，适合作为“优先按需组件、暂缓扩展完整包矩阵”的讨论输入；这不足以直接决定删除任何平台或后端。

这些数是 GitHub 资产累计下载计数，包含重复下载和可能的自动化访问，不是独立用户、成功安装或功能使用量；发布后可供下载的时长不同，且正式版可能由同一批预发布资产晋升，计数不等于“正式期新增下载”。官网/CDN、网盘、软件内其他渠道不在本表内（Java `docs/PACKAGES.md` 也明确 GitHub 数不包含官网下载）。不以三版小样本、分卷数或某版低数直接裁撤 GPU 变体；维护者需结合分发渠道与真实使用反馈定级。
