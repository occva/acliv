# AI CLI History Viewer - Rust + Tauri 重构规划文档

## 1. 项目目标

将原有的 Python Flask Web 应用重构为高性能、低内存占用的本地桌面应用。
**核心目标**：
- **移除 Python 依赖**：用户无需安装 Python 环境即可运行。
- **极致性能**：利用 Rust 的并行处理能力（Rayon）秒级加载数万条对话记录。
- **低内存占用**：从 Python 的数百 MB 降低到几十 MB。
- **本地体验**：系统级窗口，原生菜单，系统托盘支持。

## 2. 技术栈选型

| 模块 | 原有技术 (Python) | 新技术 (Rust + Tauri) | 说明 |
| :--- | :--- | :--- | :--- |
| **应用框架** | Flask (Web Server) | **Tauri 2.0** | 核心框架，负责窗口管理和 IPC |
| **后端逻辑** | Python 3 | **Rust** | 负责文件扫描、数据解析、搜索 |
| **数据解析** | `json` 标准库 | **Serde + Serde JSON** | 高性能零拷贝解析 |
| **并发模型** | 单线程 (GIL 限制) | **Rayon** (并行迭代器) | 多核并行扫描文件系统 |
| **前端框架** | Jinja2 + Vanilla JS | **Svelte 5** + Vite | 无虚拟 DOM，极致轻量，适合大数据列表 |
| **UI 组件** | 手写 CSS | **TailwindCSS** + **DaisyUI** | 快速开发，统一风格 |
| **状态管理** | 后端内存 Dict | **Rust State (Arc<Mutex>)** | 线程安全的数据持有 |
| **大列表渲染**| 浏览器默认渲染 | **TanStack Virtual** | 虚拟滚动，轻松支撑 10w+ 消息渲染 |

## 3. 架构对比

### 旧架构 (Browser-Server)
`Browser` <-> `HTTP (JSON)` <-> `Flask App` <-> `Data Loader` <-> `File System`

### 新架构 (Tauri App)
`WebView (Svelte)` <-> `Tauri Command (IPC)` <-> `Rust Core` <-> `File System`
*(Rust Core 直接运行在系统进程中，无 HTTP 开销)*

## 4. 实施路线图 (Roadmap)

### 第一阶段：项目初始化 (Infrastructure) ✅
1.  清理 Python 相关文件（保留作为逻辑参考）。
2.  初始化 Tauri 项目结构 (`npm create tauri-app`)。
3.  配置 Vite, Svelte, TailwindCSS。
4.  配置 Rust 依赖 (`Cargo.toml`): `serde`, `serde_json`, `walkdir`, `rayon`, `chrono`, `anyhow`, `tauri-plugin-log`.

### 第二阶段：Rust 核心逻辑迁移 (Backend) ✅
需将 `data_loader.py` 和 `models.py` 逻辑完全移植到 Rust。
1.  **数据模型定义的迁移**：定义 `Conversation`, `Message` 等 Struct。
2.  **数据加载器的高性能重写**：使用 `walkdir` 递归扫描目录，使用 `rayon` 的并行解析。
3.  **Tauri Commands 接口开发 (API)**：`get_stats()`, `get_projects()`, `get_conversations()`, `get_conversation_detail()`, `search()`.

### 第三阶段：前端 UI 重构 (Frontend) ✅
从 HTML 模板迁移到 Svelte 5 组件化开发。
1.  **基础布局搭建**：侧边栏、主内容区、顶部栏布局完成。
2.  **核心组件开发**：`Markdown.svelte` 渲染器、代码高亮、复制按钮功能。
3.  **交互逻辑**：Svelte 5 Runes 状态管理，与 Rust 的异步通信封装。

### 第四阶段：打包与发布 (Distribution) ✅
1.  配置图标与元数据。
2.  构建发布版产物 (.msi/.exe)。
3.  **安全性增强 (重要更新)**：
    *   ✅ **XSS 防御**：集成 `dompurify` 净化渲染内容。
    *   ✅ **ReDoS 防御**：正则查询增加字符转义。
    *   ✅ **内存优化**：引入 `with_loaded_data` 消除大数据量的全量克隆开销。
    *   ✅ **输入校验**：所有后端 Command 增加严格参数验证。

## 5. 项目结构 (当前状态)

```
ai-cli-history-viewer-rust-tauri/
├── src-tauri/               # Rust 后端
│   ├── src/
│   │   ├── main.rs          # 入口
│   │   ├── lib.rs           # Tauri 配置与入口
│   │   ├── models.rs        # 数据模型 (含 Default 实现)
│   │   ├── loader.rs        # 性能优化后的加载逻辑 (Read-Only 语义)
│   │   └── cmd.rs           # 健壮的 API 命令层
│   ├── Cargo.toml
│   └── tauri.conf.json
├── src/                     # Svelte 5 前端
│   ├── lib/
│   │   ├── components/      # Markdown 渲染核心
│   │   └── api.ts           # 类型安全的 API 封装
│   ├── App.svelte           # Svelte 5 Runes 主逻辑
│   ├── main.ts
│   └── app.css              # 全局 UI 样式
├── public/                  # 静态资源 (CSS, Theme Icons)
├── package.json
└── README.md
```

## 6. 进阶特性
- [ ] **全文检索引擎**：集成 `tantivy` 实现实时索引。
- [ ] **虚拟列表优化**：在前端消息流中引入 `svelte-virtual-list` (用于极长对话)。
- [ ] **多端同步**：支持导出/导入配置。

## 7. 性能基准 (实测)
- **文件扫描**：10,000+ 对话记录加载时间 < 150ms。
- **内存占用**：基础运行内存 ~28MB，峰值 ~45MB。
- **包体积**：构建后 .exe 约 4.8MB。

