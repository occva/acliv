# ACLIV 发布自动化方案

## 1. 摘要

这是一份面向当前仓库的发布自动化落地文档，目标是统一：

- 本地检查
- 版本升级
- 桌面端构建
- Web 构建
- GitHub Release 上传
- GHCR 镜像发布

第一版不追求复杂抽象，优先稳定、可执行、可维护。

当前方案固定为：

- 本地脚本统一入口
- 轻量 git hook 做提交前保底
- GitHub Actions 负责正式 release
- Codex skill 暂不纳入第一版正式链路，只列为后续增强

## 2. 当前现状

### 2.1 已有能力

- 当前项目已有桌面端构建链路：`npm run tauri build`
- 当前项目已有 Web 构建链路：`cargo build --release --manifest-path src-tauri/Cargo.toml --no-default-features --features web --bin acliv-web`
- 当前项目已有 Docker 发布 workflow：`.github/workflows/docker-release.yml`
- 当前项目已有 GHCR 镜像发布路径，镜像名为 `ghcr.io/occva/acliv`

### 2.2 当前缺口

当前项目还没有：

- 统一本地发布脚本
- 提交前 hook
- 桌面端 GitHub Release workflow

### 2.3 当前约束

- 当前版本文件分散在：
  - `package.json`
  - `src-tauri/Cargo.toml`
  - `src-tauri/tauri.conf.json`
- 当前 `release/` 目录仅作本地发布产物缓存，且被 `.gitignore` 忽略
- 当前发布仍依赖人工执行 `build / tag / gh release`

### 2.4 结论

第一版自动化不推翻现有 Docker workflow，只补齐：

- 本地脚本体系
- 轻量 hook
- 桌面端 release workflow

## 3. 目标架构

本方案采用三层自动化结构。

### 3.1 本地脚本层

新增 3 个脚本，作为标准入口：

- `scripts/check.ps1`
- `scripts/build-release.ps1`
- `scripts/publish-release.ps1`

职责固定为：

- `check.ps1`
  - 只做检查
  - 不改 repo 文件
- `build-release.ps1`
  - 做版本同步
  - 清理旧构建
  - 重新构建
  - 复制产物
  - 生成 release note
- `publish-release.ps1`
  - 做 tag
  - 做 push
  - 做 GitHub Release 上传

### 3.2 Git hook 层

新增轻量 hook 模板：

- `.githooks/pre-commit`
- `.githooks/pre-push`

职责固定为：

- `pre-commit`
  - 阻止误提交 `dist/`、`release/`、`src-tauri/target/`
  - 对应改动触发快速检查
- `pre-push`
  - 运行较重的发布前验证

明确限制：

- hook 不负责编译桌面包
- hook 不负责创建 tag
- hook 不负责上传 GitHub Release
- hook 不负责构建 docker image

### 3.3 CI / Release 层

保留并扩展现有 workflow：

- 保留：`.github/workflows/docker-release.yml`
- 新增：`.github/workflows/release.yml`

职责固定为：

- `release.yml`
  - 由 `v*` tag 触发
  - 构建桌面产物
  - 上传 GitHub Release 资产
- `docker-release.yml`
  - 继续负责 GHCR 镜像
  - 与桌面 release 共用同一个 tag 版本号

## 4. 脚本接口定义

本节直接锁定脚本参数与行为，避免实现阶段二次讨论。

### 4.1 `scripts/check.ps1`

建议参数：

- `-Scope local`
- `-Scope push`
- `-Scope release`

行为固定：

- `local`
  - `npx svelte-check`
  - `cargo check --manifest-path src-tauri/Cargo.toml`
- `push`
  - 包含 `local`
  - 再加 `cargo test --manifest-path src-tauri/Cargo.toml`
- `release`
  - 包含 `push`
  - 再加 `cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --features web`

约束：

- 任意一步失败即返回非零退出码
- 不自动修复问题
- 不执行会修改 repo 文件的命令

### 4.2 `scripts/build-release.ps1`

建议参数：

- `-Version 1.0.3`

行为固定：

1. 校验工作区干净，或给出明确提示
2. 同步版本到：
   - `package.json`
   - `src-tauri/Cargo.toml`
   - `src-tauri/tauri.conf.json`
3. 清理旧构建：
   - `dist/`
   - `src-tauri/target/release`
4. 执行构建：
   - `npm run tauri build`
   - `cargo build --release --manifest-path src-tauri/Cargo.toml --no-default-features --features web --bin acliv-web`
5. 复制产物到 `release/<version>/`
6. 生成：
   - `release/<version>/release-notes-v<version>.md`

约束：

- 负责生成本地发布产物
- 不负责 git commit
- 不负责 tag
- 不负责 GitHub Release 上传

### 4.3 `scripts/publish-release.ps1`

建议参数：

- `-Version 1.0.3`

行为固定：

1. 校验 `gh auth status`
2. 校验本地产物存在
3. 校验本地 tag 是否存在，不存在则创建
4. 推送：
   - `git push`
   - `git push origin v1.0.3`
5. 创建 GitHub Release
6. 上传资产：
   - `release/<version>/acliv-v<version>.exe`
   - `release/<version>/acliv-v<version>-x64-setup.exe`
   - `release/<version>/acliv-v<version>-x64-en-us.msi`

约束：

- 发布前必须基于 `build-release.ps1` 已产出的本地文件
- 失败时保留错误输出，不静默吞掉
- 不负责重新编译
- `publish-release.ps1` 只上传标准命名文件，不直接上传 `src-tauri/target/release/bundle/**` 下的原始产物

## 5. Hook 规则

### 5.1 `pre-commit`

只做轻检查：

- 检查是否误提交：
  - `dist/`
  - `release/`
  - `src-tauri/target/`
- 如果改了 `src/**` 或 `public/**`
  - 跑 `npx svelte-check`
- 如果改了 `src-tauri/**`
  - 跑 `cargo check --manifest-path src-tauri/Cargo.toml`
- 如果改了任意版本文件
  - 校验三处版本一致：
    - `package.json`
    - `src-tauri/Cargo.toml`
    - `src-tauri/tauri.conf.json`

明确不做的事：

- 不运行 `npm run tauri build`
- 不创建 release
- 不构建 docker image

### 5.2 `pre-push`

发布前保底检查：

- `cargo test --manifest-path src-tauri/Cargo.toml`
- `cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --features web`

明确不做的事：

- 不执行桌面端完整打包
- 不创建 tag
- 不上传 release
- 不发布镜像

## 6. GitHub Actions 策略

### 6.1 桌面端 `release.yml`

触发：

- `push tags: v*`

步骤固定为：

1. `checkout`
2. `setup node`
3. `setup rust`
4. 运行发布级检查
5. 构建桌面端产物
6. 上传 GitHub Release 资产

release body 来源：

- 优先读取 `release/<version>/release-notes-v<version>.md`
- 或由发布脚本生成后作为输入传入

上传资产范围：

- `acliv-v<version>.exe`
- `acliv-v<version>-x64-setup.exe`
- `acliv-v<version>-x64-en-us.msi`

### 6.2 Web / Docker `docker-release.yml`

保持现有基础逻辑，但策略要在实现中明确：

- 继续由 `master` 和 `v*` tag 触发
- 镜像标签策略：
  - `ghcr.io/occva/acliv:vX.Y.Z`
  - `ghcr.io/occva/acliv:latest`
- 容器运行必须保留：
  - `ACLIV_HOME=/app/data`
  - 保证 `search.db` 可持久化

约束：

- Docker workflow 不负责桌面端 release
- 桌面端 release workflow 不负责 GHCR 镜像
- 两者共享同一个版本号与 tag

## 7. 实施阶段

### Phase 1

- 新增 `scripts/check.ps1`
- 新增 `scripts/build-release.ps1`
- 新增 `.githooks/pre-commit`
- 新增 `.githooks/pre-push`
- 统一本地 `release/<version>/` 目录结构和产物命名
- 在 `README.md` 增加发布章节

### Phase 2

- 新增 `.github/workflows/release.yml`
- 把 GitHub Release 桌面产物发布自动化
- 与现有 `docker-release.yml` 对齐版本号和 tag 策略
- 跑一轮 `vX.Y.Z` 演练发布

### Future Work

- Codex skill 调用上述脚本，实现半自动发布助手
- skill 不作为仓库正式发布主链路依赖

## 8. 验收标准

实现完成后，至少满足以下场景：

- 修改前端后提交，`pre-commit` 会跑 `svelte-check`
- 修改 Rust 后提交，`pre-commit` 会跑 `cargo check`
- 误提交 `release/*.exe` 或 `dist/*` 会被拦截
- `scripts/check.ps1 -Scope release` 全通过
- `scripts/build-release.ps1 -Version 1.0.3` 能产出本地发布包
- `scripts/publish-release.ps1 -Version 1.0.3` 能创建 tag 并发布 GitHub Release
- `release/v1.0.3/` 目录下只保留该版本的标准命名产物，不保留临时重命名缓存
- 推送 `v1.0.3` 后：
  - 桌面 release workflow 成功
  - Docker workflow 成功
- 容器重建后索引库仍保留，验证 `ACLIV_HOME=/app/data` 生效

## 9. 默认假设

- 本文档只针对当前仓库，不做跨项目通用化
- 第一版正式方案采用“脚本 + 轻 hook + tag 驱动 CI”
- `release/` 继续忽略，不纳入 git
- 桌面产物仍以 Windows 为优先交付目标
- Codex skill 暂不进入正式发布主链路，只保留为后续增强项
