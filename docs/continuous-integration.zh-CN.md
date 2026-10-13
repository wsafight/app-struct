# 仓库质量门禁

`CI` 工作流在 Pull Request、推送 `main`、每周一 UTC 02:23 和手动触发时运行。
版本标签发布复用同一份 `quality.yml` 的完整模式；所有门禁通过后，才构建五个平台的归档并创建草稿 Release。

| 门禁 | Pull Request | 主分支、每周、手动、发布 |
| --- | --- | --- |
| Rust 格式、Clippy、workspace 测试、依赖策略与 Shell 清理回归 | 是 | 是 |
| minimal/dashboard/saas 生成 Web 的依赖检查、格式、lint、类型、测试与构建 | 三套全部 | 三套全部 |
| 文档类型、构建、链接与桌面/移动浏览器测试 | 是 | 是 |
| PostgreSQL 迁移、结构拉取与授权契约 | 是 | 是 |
| PostgreSQL 查询与组合运维/浏览器回归 | 是 | 是 |
| Dashboard 浏览器、Tenant、Audit、Mail、Jobs/Webhooks/Realtime、File、SaaS、批量与 Chromium 报表 | — | 是 |
| Docker 首次部署与渲染器隔离 | — | 是 |
| Crate 打包与开发/API 性能基准 | — | 是 |

每个 PostgreSQL 矩阵任务都有自己的 PostgreSQL 17 服务和独立测试数据库。测试可以重置该库的
`public` schema，不使用部署凭据或已有应用数据库。浏览器测试先安装固定版本的 Chromium。
CI 为后端冷启动保留最多十分钟；本地脚本保留原有默认值，除非设置 `APPSTRUCT_E2E_STARTUP_TIMEOUT`。

只缓存 Cargo registry 下载和 pnpm store，生成项目与后端 target 仍在退出时回收。
共享 runner 上的编译器/生成器测试使用 `APPSTRUCT_PERF_BUDGET_MULTIPLIER=2`；完整开发/API
基准保留各自脚本中的预算。

## 排查失败

PostgreSQL 和部署任务上传进程日志以及可用的截图/trace，保留七天。性能基准的 JSON、Prometheus
快照和进程日志保留三十天。文档浏览器失败截图保留七天。测试在进程启动前失败时，构建输出仍在
Actions 任务日志中。

E2E 清理会在删除临时项目之前复制顶层普通 `*.log` 文件，不打包环境文件、会话 Cookie、报表或整个
项目。Docker 部署会在停止容器前捕获 Compose 日志。本地可以用同一机制保留进程日志：

```bash
APPSTRUCT_E2E_LOG_DIR="$PWD/output/e2e-logs" \
APPSTRUCT_E2E_DATABASE_URL=postgresql://user@localhost/appstruct_operations_e2e \
  bash scripts/run-operations-e2e.sh
```

该命令必须使用一次性测试数据库。`scripts/test-e2e-cleanup.sh` 不连接 PostgreSQL，可验证日志保留、
成功及失败退出码、符号链接排除和临时 target 回收。

## 发布与分支保护

发布工作流等待完整门禁通过才构建归档；每个平台仍执行归档校验和与原生安装器测试，公开发布由维护者
单独执行。Docker 部署门禁在迁移和生产构建之后运行 `deploy verify`，确保校验清单包含本次交付的迁移文件。

需要强制合并前检查时，在 GitHub 分支保护中配置 required checks。提交工作流不会修改仓库设置。
Linux 容器验收和五平台安装器矩阵需要实际 Actions 执行结果，本地原生测试不能替代这些结果。
