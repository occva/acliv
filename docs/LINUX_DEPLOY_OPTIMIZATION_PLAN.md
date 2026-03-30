# Linux 部署优化方案

## 1. 目标

Linux 生产部署收敛为一条明确路径：

1. 只使用远端预构建镜像 `ghcr.io/occva/acliv:latest`
2. 目标机器不执行前端或 Rust 源码编译
3. 运行时按 provider 精确挂载历史数据目录
4. 对 `/root` 场景提供可诊断的权限处理方案

这次调整解决的是两个独立问题：

- 部署问题：低配置机器本地 `docker build` 容易卡死或 OOM
- 读取问题：容器内非 root 进程无法读取宿主机 `/root` 下的会话目录

## 2. 最终原则

### 2.1 生产部署原则

- 对外只暴露 `install.sh` 这一条正式部署入口
- 生产运行最终落到 `docker compose pull + up -d`
- 生产 compose 只使用 `image:`，不包含 `build:`
- 安装脚本不再提供源码构建 fallback
- Dockerfile 仅用于 CI 构建和发布 GHCR 镜像

### 2.2 数据读取原则

- 不再挂整个宿主机 home 目录
- 只挂载 provider 根目录
- 后端按 provider 级环境变量解析真实数据目录
- 入口脚本在启动前检查目录是否存在、是否可读
- 当数据目录位于 `/root` 下时，允许显式启用 root 运行

## 3. 当前落地结果

### 3.1 远端镜像部署

生产 compose 使用：

```yaml
image: "${ACLIV_IMAGE:-ghcr.io/occva/acliv}:${ACLIV_VERSION:-latest}"
```

默认镜像即：

```text
ghcr.io/occva/acliv:latest
```

安装脚本行为已经收敛为：

```bash
docker compose -f docker-compose.yml pull
docker compose -f docker-compose.yml up -d
```

不再执行：

```bash
docker compose up -d --build
```

### 3.2 provider 精确挂载

compose 使用以下宿主机路径变量：

- `CLAUDE_DIR`
- `CODEX_DIR`
- `GEMINI_DIR`
- `OPENCLAW_DIR`
- `OPENCODE_DIR`

容器内部统一映射到：

- `/host-data/claude`
- `/host-data/codex`
- `/host-data/gemini`
- `/host-data/openclaw`
- `/host-data/opencode`

后端读取的环境变量为：

- `ACLIV_CLAUDE_DIR=/host-data/claude/projects`
- `ACLIV_CODEX_DIR=/host-data/codex/sessions`
- `ACLIV_GEMINI_DIR=/host-data/gemini/tmp`
- `ACLIV_OPENCLAW_DIR=/host-data/openclaw/agents`
- `ACLIV_OPENCODE_DIR=/host-data/opencode/storage`

### 3.3 权限诊断

入口脚本会在启动前检查 provider 目录可读性。

如果目录不可读：

- 日志会明确指出是哪一个 provider 路径失败
- `/root` 场景下可以通过 `ACLIV_RUN_AS_ROOT=1` 保留 root 运行

后端 provider 扫描器也不再静默吞掉 `read_dir` 失败，而是输出 warning，避免前端只看到“没有数据”。

## 4. 为什么不能再让 Linux 本地构建

低配置机器的问题不是构建参数没调好，而是生产部署路径本身错了。

本地源码构建会引入这些风险：

- `npm ci` 和前端打包占用较高内存
- Rust release 编译耗时长
- 弱网络环境下依赖拉取慢
- 每次升级都重复编译
- Docker build 失败会直接阻断上线

因此 Linux 生产环境必须把“编译”和“运行”拆开：

- CI 负责构建并推送镜像
- 服务器只负责拉取和运行镜像

## 5. 镜像发布方案

GitHub Actions 工作流负责：

1. 构建镜像
2. 推送到 `ghcr.io/occva/acliv`
3. 在 tag 发布时同时推送版本 tag 和 `latest`

当前约定：

- 分支发布可更新 `latest`
- tag 发布可生成版本镜像

生产服务器只消费 GHCR 产物，不承担构建任务。

## 6. 生产部署方式

### 6.1 安装

```bash
curl -fsSL https://raw.githubusercontent.com/occva/acliv/master/deploy/install.sh | sudo env ACLIV_REPO_BRANCH=master bash
```

说明：

- 安装脚本会自动生成 `ACLIV_TOKEN`
- 安装脚本会自动探测常见 provider 历史目录并写入 `.env`
- 安装脚本会自动执行镜像拉取和服务启动
- 安装完成后会直接输出带 token 的访问 URL

### 6.2 启动与升级

```bash
cd /opt/acliv/deploy
docker compose -f docker-compose.yml pull
docker compose -f docker-compose.yml up -d
```

### 6.3 查看日志

```bash
docker compose -f docker-compose.yml logs -f acliv-web
```

### 6.4 停止

```bash
docker compose -f docker-compose.yml down
```

## 7. 配置说明

`.env` 核心字段：

```text
ACLIV_IMAGE=ghcr.io/occva/acliv
ACLIV_VERSION=latest
ACLIV_RUN_AS_ROOT=0
CLAUDE_DIR=
CODEX_DIR=
GEMINI_DIR=
OPENCLAW_DIR=
OPENCODE_DIR=
```

说明：

- `ACLIV_IMAGE` 和 `ACLIV_VERSION` 控制拉取的远端镜像
- `ACLIV_RUN_AS_ROOT=1` 仅在必须读取 `/root` 下历史目录时启用
- provider 目录变量应直接指向宿主机对应 provider 根目录

示例：

```text
CLAUDE_DIR=/root/.claude
CODEX_DIR=/root/.codex
GEMINI_DIR=/root/.gemini
OPENCLAW_DIR=/root/.openclaw
OPENCODE_DIR=/root/.config/opencode
```

## 8. 开发与生产边界

需要明确区分：

- `install.sh` 是 Linux 对外正式部署入口，负责准备 `.env`、探测 provider 路径、生成 token、拉起服务
- `docker-compose.yml` 是安装脚本落地使用的运行配置，只拉远端镜像
- `deploy/Dockerfile` 是 CI 构建镜像用，不是服务器本地构建入口
- `docker-compose.local.yml` 和 `docker-compose.build.yml` 已移除，避免继续暴露非生产路径

不再对用户暴露“手动准备 `.env` 再 compose 启动”的半成品路径。

## 9. 结论

Linux 生产部署已经明确为：

- 只用 GHCR 远端镜像
- 不做服务器本地源码编译
- 精确挂载 provider 数据目录
- 启动前做权限检查
- `/root` 场景显式选择 root 运行或明确报错

一句话总结：

> Linux 用户只需要运行 `install.sh`；脚本负责准备配置、生成 token、拉起服务。
