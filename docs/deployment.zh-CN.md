# 部署

AppStruct 产出独立的 Rust API 二进制、静态 Web 资源和显式 PostgreSQL 迁移。本地已经能跑起来之后，用本页做不可变发布：构建产物、运行时配置、把 migrate apply 作为独立步骤，以及回滚。

CLI 不会配置生产基础设施，也不会在 API 启动时自动迁移数据库。

`database.dev.migration` 只控制 `appstruct dev`。生产后端启动始终是 unmanaged：它会连接 PostgreSQL，但从不规划、创建、校验或应用迁移。

## 构建产物

生产 Web 构建默认使用当前 origin。生成的 nginx 配置会把 `/api/` 代理到 API 服务。对于独立的 API origin，请在构建前设置 `VITE_API_URL`，因为 Vite 会把它嵌入 Web 包：

```bash
# Only for a separate API origin:
# export VITE_API_URL=https://api.example.com
appstruct check
appstruct build
appstruct generate --check
```

成功构建会产出：

```text
.appstruct/cache/backend-target/release/appstruct-generated-server
generated/web/dist/
migrations/
.appstruct/schema.snapshot.json
```

没有 `app/backend` 的遗留项目会改为产出 `appstruct-generated-backend`。

官方模板还包含生产 `Dockerfile`、`deploy/` 下的 Web 镜像、`compose.production.yaml` 以及 `deploy/smoke.mjs`。构建镜像前请运行 `appstruct build`；它会准备锁定依赖和生产 Web 包。API Dockerfile 在 Linux 构建阶段编译同一套锁定的 Rust 源码，因此 macOS 主机二进制永远不会被复制进 Linux 运行时镜像：

```bash
cp .env.example .env.production
# Set DATABASE_URL to an address reachable from the API container.
# Auth apps also need APPSTRUCT_FRONTEND_URL and APPSTRUCT_ALLOWED_ORIGIN set to the public origin.
# Set production Auth/Mail/File variables in .env.production.
appstruct build
docker compose -f compose.production.yaml build
# Run the migration release job described below before starting services.
docker compose -f compose.production.yaml up -d --wait
node deploy/smoke.mjs http://127.0.0.1:8080
```

生产 Compose 文件只包含 API 和 Web 服务。它不会启动 PostgreSQL 或运行迁移。在启动新的 API 镜像之前，从匹配的发布产物执行 `appstruct migrate status` 和 `appstruct migrate apply`。

API 以 UID 10001 运行，根文件系统只读，临时目录有界，并使用持久的 `appstruct-files` 卷。PostgreSQL 和 `/metrics` 保持内部。Web 启动会等待 API 就绪。对外发布的 Web 端口默认是 `127.0.0.1:8080`；需要时通过 Compose 环境覆盖 `APPSTRUCT_WEB_BIND` 和 `APPSTRUCT_WEB_PORT`。在部署边缘终止公共 HTTPS。安全 Auth cookie 需要 HTTPS，包括验证登录时。

nginx 提供 SPA 回退、不缓存的 `index.html`、不可变的哈希资源、16 MiB 请求限制，以及对 SSE 的无缓冲 API 响应。缺失资源和未知 API 路由返回 404。更改 File/CSV 限额时，请同步更新代理的请求体限制。API 具有感知数据库的健康检查，并在 Compose 关闭时有 60 秒排空时间。

将后端二进制和 Web `dist/` 目录作为不可变产物交付。运行迁移的发布作业还需要匹配的 AppStruct CLI、项目根标记、迁移文件和 schema 快照。不要在生产运行时内从可变分支构建。

## 运行时配置

后端从进程环境读取配置：

| 变量 | 要求 | 含义 |
| --- | --- | --- |
| `DATABASE_URL` | 必需 | PostgreSQL 连接 URL |
| `APPSTRUCT_DB_MAX_CONNECTIONS` | 可选 | 连接池大小，默认 20 |
| `APPSTRUCT_DB_MIN_CONNECTIONS` | 可选 | 连接池下限，默认 1 |
| `APPSTRUCT_DB_CONNECT_TIMEOUT_SECS` | 可选 | 连接超时，默认 8 |
| `APPSTRUCT_DB_ACQUIRE_TIMEOUT_SECS` | 可选 | 连接池获取超时，默认 8 |
| `APPSTRUCT_DB_IDLE_TIMEOUT_SECS` | 可选 | 空闲连接超时，默认 300 |
| `APPSTRUCT_DB_MAX_LIFETIME_SECS` | 可选 | 连接最大生存期，默认 1800 |
| `APPSTRUCT_BIND` | 可选 | 监听地址，默认 `127.0.0.1:3000` |
| `RUST_LOG` | 可选 | tracing 过滤器 |
| `APPSTRUCT_ENV` | Auth 应用设为 `production` | 启用生产 Auth 默认值 |
| `APPSTRUCT_ALLOWED_ORIGIN` | Auth 在 `APPSTRUCT_ENV=production` 时必需；否则为可选 CORS 允许列表 | 精确允许的浏览器 origin |
| `APPSTRUCT_FRONTEND_URL` | `APPSTRUCT_ENV=production` 时必需 | 公共 Web origin |
| `APPSTRUCT_COOKIE_SECURE` | 生产中通常为 `true` | Auth cookie 的 Secure 属性 |
| `APPSTRUCT_SESSION_TTL_HOURS` | 可选 | 正数会话生存期，默认 720 |
| `APPSTRUCT_AUTH_MAIL_MODE` | 启用生产密码重置时为 `smtp` | Auth 邮件适配器 |
| `APPSTRUCT_SMTP_HOST` | SMTP 必需 | SMTP 中继主机 |
| `APPSTRUCT_SMTP_PORT` | 可选 | SMTP 中继端口 |
| `APPSTRUCT_SMTP_USERNAME` | SMTP 必需 | SMTP 凭据 |
| `APPSTRUCT_SMTP_PASSWORD` | SMTP 必需 | SMTP 密钥 |
| `APPSTRUCT_SMTP_FROM` | SMTP 必需 | 有效发件邮箱 |
| `APPSTRUCT_RESEND_API_KEY` | Resend Mail 必需 | 服务端 API 凭据 |

通过部署平台注入密钥。不要把 `.env`、数据库凭据或 SMTP 凭据烘焙进镜像或静态 Web 资源。在受信任的反向代理或负载均衡器上终止 TLS，并按环境要求使用受 TLS 保护的 PostgreSQL 连接。

## 发布顺序

每次发布都针对一套不可变产物执行：

1. 按服务恢复策略备份数据库。
2. 设置生产 `DATABASE_URL`，不要打印它。
3. 运行 `appstruct --project <release-root> migrate status`。
4. 将 `appstruct --project <release-root> migrate apply` 作为专用发布作业运行。
5. 用其运行时环境启动新的后端二进制。
6. 发布 `generated/web/dist/`，并对未知路径做 SPA 回退到 `index.html`。
7. 在退役先前发布之前，检查 `GET /health/live`、`GET /health/ready`、`GET /openapi.json`、启用时的登录，以及一次已授权的 CRUD 旅程。

迁移 apply 使用 advisory lock，校验有序历史和校验和，并在所有待处理迁移完成后检查实时 catalog 漂移。脏的非事务迁移、校验和不匹配、历史缺口或漂移会阻断发布，需要调查。

## 进程与网络模型

仅当运行时网络需要时，才将后端绑定到内部接口，例如 `0.0.0.0:3000`。通过提供 HTTPS、请求大小限制、访问日志和部署级超时的反向代理对外暴露。从静态主机或 CDN 提供 Web 目录，并将未知应用路径路由回 `index.html`。

生成的后端暴露 `/health/live` 用于进程存活，`/health/ready` 用于数据库 ping，以及兼容 Prometheus 的 `/metrics` 端点，包含就绪、有界 HTTP 直方图和 job attempt 指标。标签和查询见 [指标与数据库工作负载](observability.md)。成功响应包含 `X-Request-Id`；传入的请求 ID 会被保留，否则由后端创建。在就绪检查和一次基于数据库的冒烟旅程都成功之前，保留旧发布可用。

Auth、Mail 和 File 配置会在监听器开始服务请求之前校验。无效的环境值会返回启动错误和非零进程状态；它们不会 panic。将反复的启动失败视为部署配置问题，而不是存活失败。

后端处理 `SIGINT` 和 `SIGTERM`。收到任一信号后，它会启动 HTTP 优雅关闭，停止 Jobs worker，等待进行中的 job handler 和 HTTP 请求完成，然后才释放数据库连接。将平台终止宽限期配置为超过最长允许的请求和 job-handler 持续时间。

当 Mail 和 Jobs 都启用时，默认应用会启动一个过滤为 `mail.send` 的 worker；它不能领取无关的自定义 jobs。拥有自定义入口的应用可以在构造 `Application` 之前用 `AppExtensions::builder().job_handler(handler)` 注册综合 handler。如果启用了 Jobs 但没有 Mail 或已注册的 handler，启动会记录警告并让 worker 保持停止，而不是消费它无法处理的 jobs。

## 失败与回滚

如果迁移 apply 失败，不要启动新后端。保留日志和迁移历史，然后按已审阅的迁移流程修复失败的发布。永远不要修改数据库已记录迁移的校验和。

如果应用在兼容迁移之后失败，将流量路由回先前的后端和 Web 产物。AppStruct 不提供自动 down 迁移；不兼容的数据库回滚需要已审阅的前向修复或数据库还原。

## 首次部署验收

`node deploy/smoke.mjs <web-origin>` 探测存活、就绪、请求 ID、OpenAPI、Web shell、SPA 回退、资源 404 以及私有 metrics 边界。它不执行写入。在切换流量之前，使用应用预期的角色完成登录和一次已授权的 CRUD 旅程。

仓库会自动化一次性最小应用的发布构建、迁移（包括第二次空操作 apply）、真实生产 Compose 镜像、桌面/移动 Web 加载、同源 API 请求以及带修订检查的 CRUD：

```bash
APPSTRUCT_E2E_DATABASE_URL=postgresql://localhost/appstruct_deployment_test \
  bash scripts/run-deployment-e2e.sh
```

此命令会重置专用测试数据库的 public schema。在没有 Docker 的主机上，`--native` 通过测试代理检查发布二进制和生产包；它不验证 nginx、镜像兼容性或容器隔离。Linux workflow 运行真实容器，是发布前置条件。原生检查已在本地通过；容器检查需要 Linux CI。

## 下一步

- [安装](installation.zh-CN.md)：发布流程使用的 CLI
- [升级](upgrading.zh-CN.md)：同时变更生成代码和 schema 之前
- [迁移检查](migration-lint.zh-CN.md)：`migrate apply` 之前
