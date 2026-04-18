# ACLIV 中英双语实现说明

## 双语范围

- 本次只覆盖前端 UI，以及 Web 模式下用户可见的错误提示。
- 现有 Rust 搜索逻辑、搜索结果正文、snippet、provider 品牌名、项目路径、消息正文不做翻译。
- README、发布文档、截图资源、整仓注释不在本次范围内。

## Locale 与持久化

- 当前只支持 `zh` 和 `en`。
- `zh` 在当前版本明确表示简体中文。
- 语言偏好持久化 key 固定为 `acliv:locale`。
- 初始化顺序固定为：
  1. 先读取 `localStorage('acliv:locale')`
  2. 未命中时根据 `navigator.language` 自动判断，`zh*` 走中文，其余走英文
  3. 最终回退到 `zh`
- 运行时会同步更新 `document.documentElement.lang`。

## Key 命名约定

- 采用 ACLIV 自有轻量字典方案：`typed key + 本地词典 + t(key, params)`。
- key 使用扁平 dotted key，不做深层嵌套对象。
- 按功能分组，当前分组为：
  - `common.*`
  - `auth.*`
  - `actions.*`
  - `search.*`
  - `index.*`
  - `detail.*`
  - `toast.*`
  - `errors.*`
- 英文字典是 `TranslationKey` 的类型来源，中文词典必须完整覆盖同一组 key。

## 语言切换策略

- 登录前页面不提供手动语言切换入口。
- Web 登录页只依赖自动检测结果显示对应语言。
- 登录后的主界面与桌面端统一使用同一套前端字典。
- 手动切换入口放在索引面板 `Overview` 区域的操作区。
- 用户切换后立即刷新当前页面可翻译文案，并写入 `localStorage('acliv:locale')`。

## Web 错误 Code

- Web API 错误响应在 `error` 之外新增可选字段 `code`。
- 前端优先按 `code` 本地化，拿不到 `code` 时回退到原始 `error`。
- 当前稳定 code 列表：
  - `auth.invalid_credentials`
  - `auth.missing_credentials`
  - `auth.missing_token`
  - `request.bad_request`
  - `request.path_outside_provider_root`
  - `request.internal_error`
  - `feature.web_only`
  - `feature.desktop_only`

## 本次明确不做

- 不引入重量级 i18n 框架。
- 不改搜索分词、索引结构、Rust 搜索主流程。
- 不为登录前页面增加独立语言切换器。
- 不把其他仓库名称、路径、产品文案或配置键直接带入 ACLIV。
