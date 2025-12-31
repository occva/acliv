# AI CLI History Viewer (Rust + Tauri Edition)

> 🚀 **高性能桌面应用** - 使用 **Rust** 和 **Tauri** 重构的 AI CLI 对话历史查看器

## ✨ 特性

- **极速加载** - 使用 Rayon 并行处理，秒级加载数万条对话记录
- **低内存占用** - Rust 的零成本抽象，内存占用降低 5-10 倍
- **原生桌面体验** - 基于系统 WebView，无需打包 Chromium
- **多数据源支持** - Claude CLI / Codex CLI / Gemini CLI
- **暗色/亮色主题** - 精心设计的 UI，支持主题切换

## 🛠️ 技术栈

| 层级 | 技术 |
|:-----|:-----|
| **桌面框架** | Tauri 2.0 |
| **后端** | Rust (Serde, Rayon, Regex) |
| **前端** | Svelte 5 (Runes) + Vite |
| **安全** | DOMPurify (XSS 防御), Regex Escape (ReDoS 防御) |
| **样式** | Vanilla CSS (暗色/亮色主题) |

## 🛡️ 安全与鲁棒性亮点

- **XSS 防御**：前端集成 `DOMPurify` 彻底净化 Markdown 渲染产物。
- **ReDoS 防护**：搜索功能对用户输入进行正则转义（Regex Escape），防止正则注入攻击。
- **内存安全**：后端采用锁竞争优化和全量克隆消除，大数据量下依然稳健。
- **输入校验**：所有 Tauri Commands 均包含严格的长度和内容校验。
- **环境隔离**：生产环境禁用了 DevTools，保障用户数据私密性。

## 📦 开发

### 环境要求

- Node.js 18+
- Rust 1.77+
- Tauri CLI (`cargo install tauri-cli`)

### 安装依赖

```bash
# 前端依赖
npm install
```

### 开发模式

```bash
# 启动 Tauri 开发模式 (自动启动前端和后端)
cargo tauri dev

# 或者分别启动
npm run dev          # 仅前端
cargo build          # 仅后端
```

### 构建发布版

```bash
cargo tauri build
```

构建产物位于 `src-tauri/target/release/bundle/`

## 📁 项目结构

```
├── src-tauri/           # Rust 后端
│   ├── src/
│   │   ├── lib.rs       # Tauri 应用入口
│   │   ├── models.rs    # 数据模型
│   │   ├── loader.rs    # 数据加载器 (核心)
│   │   └── cmd.rs       # Tauri Commands
│   └── Cargo.toml
├── src/                 # Svelte 前端
│   ├── App.svelte       # 主组件
│   ├── app.css          # 全局样式
│   └── lib/api.ts       # API 封装
└── package.json
```

## 📊 性能对比

| 指标 | Python 版本 | Rust/Tauri 版本 |
|:-----|:------------|:----------------|
| 启动时间 | ~2s | <0.5s |
| 内存占用 | ~150MB | ~30MB |
| 打包体积 | ~50MB | ~5MB |
| 文件加载 | 单线程 | 并行处理 |

## 📄 文档

详细的重构规划请参阅 [REFACTOR_PLAN.md](./REFACTOR_PLAN.md)

## 📜 License

MIT
