# ACLIV 搜索能力升级方案

## 1. 现状

### 1.1 当前搜索实现

当前项目的搜索仍然是前端内存过滤，入口和数据链路如下：

- `src/App.svelte`
  - `handleSearchInput()` 直接对 `allSessions` 做 `filter()`
  - 过滤字段只有 `sessionId`、`title`、`summary`、`projectDir`
  - 结果类型是 `SearchResultLocal[]`，不包含命中片段、消息定位、相关性排序
- `src/lib/api.ts`
  - 目前没有 `search` 相关接口
- `src-tauri/src/cmd.rs`
  - 只有 `list_sessions`、`get_session_messages`、`delete_session` 等命令
- `src-tauri/src/session_manager/mod.rs`
  - 已具备统一的 provider 扫描和消息加载能力
  - 但没有“结构化入库”和“可复用索引层”

### 1.2 当前短板

1. 搜索范围只覆盖会话元信息，不覆盖消息正文。
2. 每次输入都在前端扫描内存数组，数据量变大后体验会快速退化。
3. 没有结果高亮、片段摘要、相关性排序、过滤器。
4. 没有持久化索引，应用重启后无法复用任何搜索预处理结果。
5. `list_sessions()` 已经需要扫描 provider 文件；如果后续把正文搜索继续放在“实时扫文件”上，性能会明显失控。

### 1.3 可借鉴点与不照搬点

`spool` 的可借鉴点是：

- 本地 SQLite 持久化
- `sources -> projects -> sessions -> messages` 的分层建模
- `messages_fts` 这类 FTS5 虚拟表
- 基于文件 `mtime` 的增量索引思路
- 搜索结果返回 snippet，而不是只返回会话名
- 后台同步、状态查询、watcher 分层拆分

不建议照搬的部分：

- Electron IPC 结构
- TypeScript + `better-sqlite3` 实现方式
- `captures/opencli` 这一类与当前项目无关的数据域

当前项目更适合在 Rust 侧建立本地索引层，并让 Tauri 桌面端与 `acliv-web` 共用同一套搜索服务。

### 1.4 与 spool 实码对齐后的基线

这里按 `spool` 当前代码确认一遍可复用模式：

- `packages/core/src/db/db.ts`
  - 建了 `sources`、`projects`、`sessions`、`messages`、`messages_fts`、`sync_log`
  - FTS 采用 `content='messages'` + trigger 同步
- `packages/core/src/db/queries.ts`
  - `searchFragments()` 直接查 `messages_fts`
  - 默认把普通查询包成 phrase 查询
  - 用 `snippet(messages_fts, -1, '<mark>', '</mark>', '…', 20)` 返回高亮片段
  - 支持 `source`、`since` 过滤
- `packages/core/src/sync/syncer.ts`
  - 先扫文件，再按 `mtime` 判断新增/更新/跳过
  - 单个 session 以事务写入：`upsertSession()` + `insertMessages()`
  - session 更新时先删旧消息，再写入新消息
- `packages/core/src/sync/watcher.ts`
  - watcher 只负责监听和 debounce，不负责复杂业务逻辑
  - 初始全量同步和实时增量同步是两条链路
- `packages/app/src/main/index.ts`
  - IPC 只暴露 `search`、`listSessions`、`getSession`、`getStatus`、`syncNow`
  - 搜索不依赖前端已加载的列表状态

这意味着对当前 Tauri 项目来说，真正需要“对齐 spool”的不是 UI，而是：

1. 数据库 schema 形状。
2. `syncer/query/status` 的模块边界。
3. 以消息片段为核心的首版搜索结果模型。
4. 基于 `mtime` 的低复杂度增量更新策略。

## 2. 目标架构

### 2.1 升级目标

把搜索能力从“前端字符串过滤”升级为“本地索引 + 全文搜索 + 渐进式迁移”，满足以下目标：

- 搜索消息正文，而不只搜索会话标题/摘要
- 支持本地持久化索引，避免每次搜索都重新扫源文件
- 搜索 API 同时服务 Tauri 桌面端和 `acliv-web`
- 初期尽量复用现有 `session_manager::scan_sessions()` / `load_messages()`
- 支持增量刷新，而不是每次全量重建
- 保留当前 UI 可用性，允许分阶段替换

### 2.2 推荐方案

推荐使用 `SQLite + FTS5`，不推荐第一阶段直接上 `tantivy`。

原因：

1. 当前项目的核心数据天然是结构化的：provider、session、message、时间、路径。SQLite 同时适合存结构化字段和全文索引。
2. 未来需要做过滤、排序、去重、状态记录、增量同步，SQLite 比纯搜索引擎更顺手。
3. `spool` 已经验证了“本地 SQLite + FTS”的产品形态，但本项目可以在 Rust 中用 `rusqlite` 实现，不依赖 Node 运行时。
4. 对桌面应用来说，单机 SQLite 的部署和排障成本最低。

### 2.3 推荐模块拆分

建议新增 `src-tauri/src/search_index/`，模块边界尽量向 `spool` 的 `db + queries + sync` 靠拢：

- `mod.rs`
  - 对外暴露 `search()`、`refresh_index()`、`rebuild_index()`、`get_index_status()`
- `db.rs`
  - 打开数据库、初始化 pragma、执行 schema migration
- `schema.rs`
  - 管理建表和版本迁移
- `indexer.rs`
  - 对应 `spool` 的 `syncer` 核心写库逻辑
  - 把 `SessionMeta + SessionMessage[]` 写入索引库
- `query.rs`
  - 对应 `spool` 的 `searchFragments()`
  - 执行 FTS 查询、过滤、snippet/highlight
- `types.rs`
  - 定义 `SearchHit`、`SearchFilters`、`IndexStatus`
- `watcher.rs`
  - 后续阶段再引入
  - 对应 `spool` 的 watcher，只做文件监听和节流
- `paths.rs` 或扩展现有 `paths.rs`
  - 统一解析索引库路径

现有 `session_manager` 继续负责“读 provider 原始数据”，新增 `search_index` 负责“结构化落库与查询”。

职责边界建议保持为：

- `session_manager`: 原始数据读取、provider 差异消化
- `search_index`: 索引状态、增量同步、FTS 查询
- `cmd.rs` / `acliv-web.rs`: 对外命令和 HTTP 接口

### 2.4 目录与运行时建议

建议引入独立索引目录，而不是把索引文件放在 provider 目录下。

建议优先级：

1. `ACLIV_INDEX_DIR` 环境变量
2. 桌面端使用 app data 目录
3. web 模式退化到 `ACLIV_HOME` 或用户 home 下的应用目录

建议文件：

- `search.db`
- `search.db-wal`
- `search.db-shm`

## 3. 分阶段实施

### Phase 0：抽象搜索接口，不改动现有体验

目标：

- 先把“搜索是前端行为”改成“搜索是后端能力”
- API 形状尽量向 `spool:search` 对齐，但保持 Tauri/web 双端复用

实施：

1. 在 Rust 侧新增 `search()` 命令。
2. `src/lib/api.ts` 增加 `search()`。
3. `App.svelte` 不再直接操作 `allSessions.filter(...)`，改成调用后端。
4. 第一版可以允许后端内部临时回退到轻量扫描，但前端不再感知这种差异。

收益：

- 前后端边界先稳定下来
- 后续切换到 FTS 时，前端改动最小

### Phase 1：引入本地索引库，支持全量构建

目标：

- 能把现有 provider 会话与消息写入本地 SQLite
- 支持真正的消息全文搜索

实施：

1. 新增 `rusqlite`
   - 建议优先评估 `bundled`，避免不同平台 SQLite 编译特性不一致
2. 实现与 `spool` 接近的 schema migration
3. 复用 `session_manager::scan_sessions()` 获取 `SessionMeta`
4. 对每个 session 调用 `load_messages()`，写入 `sessions` + `messages` + `messages_fts`
5. 新增命令：
   - `search_index_rebuild`
   - `search`
   - `get_search_status`
6. 前端搜索结果第一版只返回“消息片段命中”，和 `spool` 的 `searchFragments()` 保持一致

阶段原则：

- 第一版允许“应用启动后手动构建索引”
- 先不做 watcher
- 先把正确性和可维护性做稳
- 第一版先不做“session 元数据命中”和“混合结果排序”，避免过早复杂化

### Phase 2：增量刷新，接入现有刷新周期

目标：

- 不再每次全量重建
- 与当前 `silentRefresh()` 节奏兼容

实施：

1. 为每条 session 至少记录 `source_path`、`raw_mtime`、`last_indexed_at`
   - 这里先与 `spool` 的 `raw_file_mtime` 基线对齐
   - `file_size` 可作为本项目后续增强，不必第一版就上
2. 每次刷新时：
   - 先跑 `scan_sessions()`
   - 与索引库中的 session 状态做 diff
   - 仅重建新增/变更/已删除的 session
3. 索引写入按 session 事务化：
   - 删除旧消息
   - 写入新消息
   - 更新 session 元数据
4. `delete_session()` 成功后同步删除索引记录

结果：

- 列表刷新与搜索索引逐步收敛
- 搜索结果可持续保持新鲜

### Phase 3：搜索体验升级

目标：

- 让搜索从“能查到”升级到“可用”

实施：

1. 保持 `fragment` 结果模型，但补足体验字段
2. 支持 snippet 和 `<mark>` 高亮文本
3. 支持过滤器：
   - provider
   - project
   - 时间范围
4. 点击消息命中时：
   - 打开对应会话
   - 后续可定位到对应消息或附近消息
5. 增加索引状态 UI：
   - 是否已构建
   - 上次更新时间
   - 当前索引条数
6. 如果后续需要做 session-meta 命中，再在这一阶段引入混合结果集

### Phase 4：后台实时同步与高级能力

这阶段不建议现在就做，但架构上要预留。

可选项：

- 文件 watcher，实时同步索引
- 搜索排序增强
  - 标题命中加权
  - 最近活跃时间加权
  - 用户消息命中优先
- 混合搜索
  - 元信息命中 + 正文命中合并排序
- 搜索历史
- 高级语法
  - phrase
  - prefix
  - AND / OR

## 4. 数据模型

建议先保持最小可用模型，不要在第一版引入过多抽象。

### 4.1 表结构建议

#### `sources`

字段建议：

- `id`
- `name`
- `base_path`
- `created_at`

说明：

- 这一层直接和 `spool` 对齐
- 当前项目至少会有 `claude`、`codex`、`gemini`、`openclaw`、`opencode`

#### `projects`

字段建议：

- `id`
- `source_id`
- `slug`
- `display_path`
- `display_name`
- `last_synced`

说明：

- 这一层也建议直接沿用 `spool` 的模型
- `gemini` 的 `project_dir` 当前不可逆，可退化为 provider 分组或 hash 分组

#### `sessions`

字段建议：

- `id`
- `project_id`
- `source_id`
- `session_id`
- `source_path`
- `title`
- `summary`
- `created_at`
- `last_active_at`
- `raw_mtime`
- `message_count`
- `indexed_at`

约束建议：

- `UNIQUE(source_path)`
- 如果担心不同 provider 路径语义冲突，可退化成 `UNIQUE(source_id, source_path)`

用途：

- 存会话元数据
- 驱动增量同步
- 支持结果页的基础展示

#### `messages`

字段建议：

- `id`
- `session_id`
- `source_id`
- `seq`
- `role`
- `content`
- `ts`

索引建议：

- `(session_id, seq)`
- `(session_id, ts)`

用途：

- 保存归一化后的消息正文
- 支持会话重建与搜索结果回跳

#### `messages_fts`

建议先和 `spool` 对齐，只索引正文：

- `content`

实现建议：

- 用 FTS5 虚拟表
- 使用 contentless/content-linked 方案均可
- 第一版优先采用和 `spool` 一样的“普通表 + FTS 表 + trigger”模型，降低复杂度

#### `sync_log`

字段建议：

- `id`
- `source_id`
- `source_path`
- `status`
- `message`
- `synced_at`

说明：

- 这张表在 `spool` 里已经证明有价值
- 对当前项目尤其有用，因为 provider 数更多，排查失败文件需要落地记录

### 4.2 是否拆分 session 元数据和 message 索引

建议拆分。

原因：

1. 会话列表页和搜索命中的读取模式不同。
2. 增量重建时，session 元数据和消息明细更新频率不同。
3. 未来做“只重建某一 session 的 message 文本”时更灵活。

### 4.3 增量同步判定

第一版建议先严格对齐 `spool` 的低复杂度方案：

- `source_path` 不存在于索引库中：新增
- `raw_mtime` 变化：重建
- 扫描结果中不存在但索引库存在：删除

如果后续发现 provider 存在“内容变了但 mtime 不变”的极端情况，再补 `file_size` 或 `content_hash`。

## 5. API / UI 改造

### 5.1 Rust Command / HTTP API

建议新增以下接口：

- `search(query, filters, limit, offset)`
- `rebuild_search_index()`
- `refresh_search_index()`
- `get_search_status()`

其中：

- `search()` 同时给 Tauri IPC 与 `acliv-web` 使用
- `refresh_search_index()` 负责增量同步
- `rebuild_search_index()` 负责清库重建

首版返回类型建议先向 `spool` 的 `FragmentResult` 靠拢，而不是一开始做复杂 union：

```ts
type FragmentHit = {
  rank: number
  providerId: string
  sessionId: string
  sourcePath: string
  sessionTitle: string
  project: string
  startedAt?: number
  snippet: string
  messageRole: string
  messageTimestamp?: number
  seq?: number
}
```

后续如果要支持 session-meta 命中，再扩展成 discriminated union。

### 5.2 前端状态改造

当前：

- `searchQuery`
- `searchResults`
- `handleSearchInput()` 直接 filter

建议改成：

- `searchQuery`
- `searchResults: FragmentHit[]`
- `searchLoading`
- `searchError`
- `searchRequestToken`

关键点：

1. 输入框 debounce 150ms 到 250ms。
2. 每次发起请求时记录 token，防止旧请求覆盖新结果。
3. 空查询直接清空，不命中后端。
4. 搜索结果列表第一版只渲染 fragment 命中，但要把会话标题、项目、时间、角色信息展示完整。

### 5.3 搜索结果交互建议

第一版：

- 点击任何命中，直接打开对应会话

第二版：

- 如果是消息命中，尝试定位到对应消息
- 若当前消息详情仍按 `pairs` 渲染，可先退化为“打开会话并高亮最近匹配消息”

### 5.4 与当前列表页的关系

建议不要把搜索结果强行塞回现有 `Project -> Conversation` 两级列表。

更合适的方式：

- 搜索弹窗保持独立结果流
- 列表页继续负责浏览
- 搜索负责跨项目、跨会话、跨正文的快速直达

这样能避免把“浏览模型”和“搜索模型”混成一套状态机。

## 6. 风险与工作量

### 6.1 主要风险

#### 风险 1：首次建索引耗时较长

原因：

- 当前 provider 解析是“按需读原始文件”
- 第一次建索引需要对每个 session 执行 `load_messages()`
- `spool` 当前只覆盖 claude/codex，而本项目一次要覆盖 5 个 provider，首轮成本会更高

应对：

- 首次构建做成后台任务
- 提供状态查询和进度提示
- 支持“先可搜索元信息，再补正文索引”作为兜底策略

#### 风险 2：不同 provider 的消息结构差异较大

原因：

- `claude/codex/openclaw` 是 JSONL 流式事件
- `gemini` 是 JSON 数组
- `opencode` 的 message / part 目录是分离的

应对：

- 第一阶段直接复用现有 `load_messages()`
- 不在搜索升级里同步重写 provider parser
- 等索引能力稳定后，再考虑抽象统一的 message iterator

#### 风险 3：索引一致性问题

表现：

- 删除会话后索引残留
- provider 原文件变化但索引未更新

应对：

- `delete_session()` 成功后同步删索引
- `silentRefresh()` 或显式 refresh 时做增量同步
- session 维度事务写入，避免半更新
- watcher 只做事件触发，真正的写库仍走 `refresh_search_index()` / `sync_file()` 风格的统一入口

#### 风险 4：SQLite FTS 特性在不同平台不一致

应对：

- 评估 `rusqlite` 的 `bundled` 方案
- 在 Linux 部署文档中明确依赖策略

#### 风险 5：Tauri 与 web 模式共用路径策略

应对：

- 提前统一索引路径解析逻辑
- 不把索引目录绑定到 Tauri 专属 API

### 6.2 推荐验收标准

Phase 1 完成标准：

- 可执行全量建索引
- 能搜到消息正文关键词
- 返回带 snippet 的结果
- Tauri 与 web 模式都能调用搜索接口
- 结果结构与 `spool` 的 fragment search 保持同量级复杂度

Phase 2 完成标准：

- 删除、新增、修改 session 后，增量刷新能正确反映搜索结果
- 重启应用后无需重新全量建索引

Phase 3 完成标准：

- 搜索结果展示信息完整，用户可判断命中上下文
- 结果可跳转到目标会话
- 有基本状态提示和错误处理

### 6.3 粗略工作量评估

按当前代码基础，建议按以下量级预估：

- Phase 0：0.5 到 1 人日
  - 抽 API，前端改调用边界
- Phase 1：2 到 4 人日
  - `rusqlite` 接入、schema、全量索引、FTS 查询、基础 UI
- Phase 2：1.5 到 3 人日
  - 增量同步、删除联动、状态接口
- Phase 3：1 到 2 人日
  - 搜索结果体验、snippet 展示、结果交互

合计：

- 一个可落地、可维护、带正文搜索的版本，大约 5 到 10 人日

## 建议结论

基于当前项目现状，最稳妥的路线不是直接重做搜索 UI，也不是直接照搬 `spool` 的 Electron/TypeScript 实现，而是：

1. 先把搜索入口后移到 Rust，稳定接口边界。
2. 用和 `spool` 相近的 `sources/projects/sessions/messages/messages_fts/sync_log` 模型建索引。
3. 第一阶段复用现有 `scan_sessions()` 和 `load_messages()`，避免大改 provider。
4. 搜索结果先做成 `fragment-only`，和 `spool` 的 `searchFragments()` 保持同一复杂度。
5. 在现有 `silentRefresh()` 基础上补增量索引刷新，再逐步补 watcher、消息定位和高级搜索体验。

这条路线和当前 Tauri + Rust + Svelte 架构是对齐的，改动面可控，也最容易收敛成真正能上线的版本。

## 7. 当前完成度对照

以下完成度基于当前仓库代码状态，而不是方案目标值。

### 7.1 Phase 完成情况

- Phase 0：已完成
  - 前端搜索已切到后端接口
  - `src/lib/api.ts`、Tauri command、web API 已统一
- Phase 1：已完成
  - 本地 `SQLite + FTS5` 已落地
  - 已有 `sources / projects / sessions / messages / messages_fts / sync_log`
  - 支持全量建库、消息正文全文搜索、snippet 返回
- Phase 2：已完成
  - 已支持 `refresh_index()` 增量刷新
  - 已按 `source_path + raw_mtime + raw_size` 做新增/更新/删除
  - `delete_session()` 已联动删除索引
- Phase 3：基本完成
  - 搜索结果已支持 snippet、高亮、命中跳转、消息定位
  - 已支持 provider、project、时间范围过滤
  - 已有索引状态卡片和手动 `Sync / Rebuild`
- Phase 4：部分完成
  - 桌面端 watcher 已接入，文件变更可触发自动 refresh
  - 高级排序、混合结果、搜索历史、高级语法仍未实现

### 7.2 数据模型完成情况

已完成：

- `sources`
- `projects`
- `sessions`
  - 已有 `provider_session_id`、`source_path`、`title`、`summary`
  - 已有 `cwd`、`model`
  - 已有 `created_at`、`last_active_at`、`message_count`
  - 已有 `raw_mtime`、`raw_size`、`indexed_at`
  - 已补 `resume_command`、`has_tool_use`
- `messages`
  - 已有 `seq`、`role`、`content_text`、`ts`
  - 已补 `msg_uuid`、`parent_uuid`、`is_sidechain`、`tool_names`
- `messages_fts`
- `sync_log`

仍未完全对齐 `spool` 的部分：

- 更可靠的 `is_sidechain/tool_names` provider 级解析

说明：

- 当前 schema 已支持这些扩展继续落库
- `cwd/model` 已进入索引读链路
- 但 provider parser 侧目前只对部分字段做 best-effort 提取，未做大规模重写

### 7.3 UI / API 完成情况

已完成：

- Tauri 与 web 共用搜索接口
- 内容搜索、索引状态、全量重建、增量刷新、索引会话列表、索引消息读取
- 列表页已切为索引优先
- 详情读取已切为索引优先
- 前端对原始扫描的 metadata fallback 已明显收缩，不再为常规详情行为二次扫 provider
- 搜索弹窗已支持过滤和结果定位

未完成：

- 基于索引的“最近会话首页”独立视图
- 更细粒度的同步进度展示
- 搜索结果混合排序

### 7.4 总体判断

如果按方案文档本身衡量：

- 核心可交付目标已经完成，大约可视为 `80% - 85%`
- “本地索引 + 全文搜索 + 渐进式迁移”主线已经落地
- 剩余工作主要不是基础能力缺失，而是高级体验和更深的 provider 元数据抽取

如果按与 `spool` 的搜索主线对齐程度衡量：

- 已对齐的核心能力：
  - 本地 SQLite
  - FTS 搜索
  - 增量刷新
  - watcher 自动同步
  - snippet 命中
  - 状态查询
- 尚未完全对齐的增强能力：
  - 更完整 message/session 元数据沉淀
  - 更成熟的同步状态表达
  - 更丰富的排序与结果组织
