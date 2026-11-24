# AppStruct

[English](README.md) | [简体中文](README.zh-CN.md)

**面向 AI Coding Agent 的确定性全栈编译器 —— 同时让存量 PostgreSQL 3 分钟生成安全合规管理后台。**

> **你（或你的 AI Agent）只需编写声明式 YAML，无需精通 Rust 即可交付高性能全栈应用。**

AppStruct 将多文件声明式 YAML App Spec 确定性编译为类型化中间表示（IR）、向前兼容的 PostgreSQL 迁移、默认拒绝（Fail-Closed）的 Rust（Axum/SQLx）后端、OpenAPI 契约、TypeScript 客户端以及 React 19 / Rsbuild 前端应用。

---

## 为什么选择 AppStruct？

### 1. 终结 AI 写全栈代码时的“权限漏校验、租户越权与迁移事故”
当 AI Coding Agent 直接手写全栈 CRUD、SQL 迁移和鉴权中间件时，极易遗漏租户隔离、泄露敏感字段或生成锁表 DDL。

AppStruct 将 AI 全栈开发变成**可静态验证的确定性编译闭环**：

```text
AI 编写声明式 YAML Spec
  → 内置 MCP Server (`appstruct mcp`) + Draft 2020-12 JSON Schema
  → `appstruct check --deny-warnings` + `appstruct access preview` 精确报错与权限预演
  → 确定性编译生成安全 Rust + PostgreSQL + React/TS 应用
```

### 2. 存量 PostgreSQL 3 分钟生成安全管理后台
已有现成的 PostgreSQL 数据库？用 `appstruct db pull` 只读拉取表结构生成默认拒绝的 App Spec 草稿，审核权限规则后即可交付单二进制 Rust + React 管理后台 —— **无按席位收租、无供应商锁定、完整源码可控**。

```bash
appstruct db pull --schema public --output spec/imported.yaml
appstruct access preview --role admin
appstruct migrate baseline --accept
appstruct dev
```

---

## 一条命令体验完整 Demo（主机零依赖）

无需在本地安装 Rust、Node.js、pnpm 或 PostgreSQL，只需 Docker：

```bash
docker compose -f docker-compose.demo.yaml up --build
```

启动后访问 **http://localhost:8080**，即可体验完整的多租户 SaaS 示例（认证、组织租户隔离、RBAC、审计日志、项目/任务管理、Jobs/Outbox 与运维控制台）。

---

## 接入 AI Coding Agent（内置 MCP + Skill + Rules）

AppStruct 内置**零外部依赖的 stdio MCP Server**（`appstruct mcp`），并自带开箱即用的 Agent 规则与技能包：

- **Claude Code**：
  ```bash
  claude mcp add appstruct -- appstruct mcp
  ```
- **Cursor / Windsurf / Antigravity / Codex**：
  - MCP 启动命令：`appstruct mcp`（可选 `--project /path/to/app`）
  - 仓库内置规则与技能：[`.cursor/rules/appstruct.mdc`](.cursor/rules/appstruct.mdc)、[`.claude/skills/appstruct/SKILL.md`](.claude/skills/appstruct/SKILL.md)，以及每个生成项目自带的 `AGENTS.md`。

**暴露的 MCP 工具：**
- `appstruct_schema` — 获取 Draft 2020-12 App Spec JSON Schema（支持按 `section` 过滤）
- `appstruct_check` — 校验 Spec、IR 与用户扩展符号，返回结构化诊断信息
- `appstruct_access_preview` — 预演指定角色/主体下的 CRUD、字段级、Owner 与租户权限矩阵
- `appstruct_generate` — 原子化重新生成代码（支持 `check: true` 漂移检测）
- `appstruct_migrate_plan` — 对比 schema 快照、预览迁移 SQL 并执行迁移 Lint
- `appstruct_db_pull_preview` — 只读内省实时 PostgreSQL 表结构并预览生成的 Spec 草稿

---

## 本地环境要求与安装

| 依赖 | 版本要求 | 用途 |
| --- | --- | --- |
| Rust | `>= 1.89`（Edition 2024，含 `rustfmt` 与 `clippy`） | CLI 与生成后端 |
| PostgreSQL | 推荐 `16` / `17` | 数据库迁移与运行时 API |
| Node.js | `>= 20.0.0` | 生成 Web 应用 |
| pnpm | `>= 9.0.0` | Web 依赖安装 |
| Docker Compose | 可选 | 仅用于 `database.dev.mode: managed` 或一键 Demo |

从源码构建 CLI：

```bash
cargo build --release --locked -p appstruct-cli
install -d "$HOME/.local/bin"
install -m 0755 target/release/appstruct "$HOME/.local/bin/appstruct"
appstruct --version
```

> 希望零配置本地开发？可用 VS Code 打开本仓库并安装 **Dev Containers** 扩展，选择 **Reopen in Container**。详情见[安装指南](docs/installation.zh-CN.md)。

---

## 快速开始

### 创建新应用（`minimal` / `dashboard` / `saas`）

```bash
# 多租户 SaaS 预设（含 Auth、Tenant、Audit、Mail、Jobs、File）+ 托管 PostgreSQL
appstruct init my-saas --template saas
cd my-saas
appstruct doctor
appstruct dev
```

或接入**已有 PostgreSQL 数据库**：

```bash
appstruct init my-admin --template minimal --database-mode external
cd my-admin
cp .env.example .env # 配置 DATABASE_URL
appstruct db pull --schema public --output spec/imported.yaml --access role --role admin
# 将 spec/imported.yaml 加入 appstruct.yaml 的 includes 后执行：
appstruct check --deny-warnings
appstruct migrate baseline --accept
appstruct dev
```

默认本地访问地址：
- Web 界面：`http://127.0.0.1:5173`
- API 与 OpenAPI：`http://127.0.0.1:3000`（`/openapi.json`）

在可信主机初始化首个管理员账号：

```bash
appstruct auth bootstrap-admin --email admin@example.com
```

---

## 核心架构与工程保障

- **默认拒绝（Fail-Closed）权限模型**：每个实体必须显式声明 `list`、`read`、`create`、`update`、`delete` 规则（`public`、`authenticated`、`roles`、`owner`），支持字段级读写隔离与租户隔离，已通过 2,112 项 PostgreSQL 权限真值表验证。
- **清晰的代码所有权边界**：
  - **用户完全掌控**：`appstruct.yaml`、`spec/*.yaml`、`app/backend/`（自定义 Rust Policy、Hook、`AfterCommitHook`、路由）、`app/web/`（自定义 React 组件与页面）、`migrations/*.sql`。
  - **编译器原子管理**：`generated/`（SHA-256 校验，原子替换，禁止手改）。
- **安全的在线迁移**：提供 `migrate plan`、`migrate lint --deny-warnings`、`migrate dev --accept`、`migrate baseline --accept`，生产环境显式执行 `migrate apply`。
- **现代全栈技术基线**：
  - **后端**：Rust、Axum、SQLx、`sift-query`、乐观并发控制（`If-Match: "rev-<n>"`）、偏移与游标分页、关系过滤、聚合与分组查询、实体工作流、签名 Webhook、Jobs/Outbox、定时任务。
  - **前端**：React 19、TypeScript、Rsbuild/Rspack、Rstest、TanStack Query / Router / Table / Form。

---

## 常用 CLI 命令

```text
appstruct init [<name>] [--template minimal|dashboard|saas] [--database-mode external|managed]
appstruct mcp                                         # 启动内置 stdio MCP Server
appstruct schema                                      # 输出 Draft 2020-12 App Spec JSON Schema
appstruct check [--deny-warnings] [--format json]     # 校验 Spec、IR 与用户扩展符号
appstruct access preview [--entity <id>] [--role <r>] # 预演 CRUD、字段与租户权限矩阵
appstruct generate [--check] [--timings]              # 重新生成后端、前端、OpenAPI 与 IR
appstruct migrate plan|lint|dev|baseline|apply|status # 向前兼容 PostgreSQL 迁移管理
appstruct db pull [--schema public] [--review|--diff] # 只读内省已有 PostgreSQL 生成 Spec
appstruct dev [--api-port 3000] [--web-port 5173]     # 启动协调式本地开发服务器
appstruct build                                       # 生产构建并输出发布清单
appstruct deploy verify                               # 校验不可变发布清单
appstruct doctor [--format json]                      # 本地环境与数据库诊断
appstruct auth bootstrap-admin --email <address>      # 初始化首个管理员
appstruct preset show [--expanded]                    # 查看锁定预设配置
```

---

## 文档导航

所有指南均提供英文与简体中文版本（`*.zh-CN.md`），可在根目录运行 `pnpm run site:dev` 本地预览文档站。

- **入门上手**：[安装指南](docs/installation.zh-CN.md) · [已有数据库指南](docs/existing-database.zh-CN.md) · [升级指南](docs/upgrading.zh-CN.md)
- **数据建模与查询**：[资源查询](docs/data-querying.zh-CN.md) · [Schema 索引](docs/schema-indexes.zh-CN.md) · [Seed 数据](docs/seeding.zh-CN.md) · [软删除与历史](docs/soft-delete.zh-CN.md) · [保存视图](docs/saved-views.zh-CN.md)
- **业务逻辑与 UI**：[实体工作流](docs/workflows.zh-CN.md) · [批量操作](docs/bulk-operations.zh-CN.md) · [无头控制器](docs/headless-controller.zh-CN.md) · [报表](docs/reports.zh-CN.md) · [记录动态](docs/activity.zh-CN.md)
- **平台模块**：[OAuth 与 OIDC](docs/oauth-oidc.zh-CN.md) · [组织邀请](docs/organization-invitations.zh-CN.md) · [个人 API Token](docs/personal-api-tokens.zh-CN.md) · [签名 Webhook](docs/webhooks.zh-CN.md) · [定时任务](docs/schedules.zh-CN.md) · [实时事件](docs/realtime.zh-CN.md) · [计费模块](docs/billing.zh-CN.md)
- **部署与运维**：[部署指南](docs/deployment.zh-CN.md) · [迁移 Lint](docs/migration-lint.zh-CN.md) · [运维控制台](docs/admin-console.zh-CN.md) · [可观测性](docs/observability.zh-CN.md) · [CI 质量门禁](docs/continuous-integration.zh-CN.md)
- **架构与路线图**：[产品路线图](docs/next-product-roadmap.zh-CN.md) · [产品基线](PRODUCT.md) · [技术设计](TECHNICAL_DESIGN.md)

---

## 质量检查

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
scripts/run-template-build.sh
```

## 许可证

Workspace 中的 crate 使用 **MIT OR Apache-2.0** 双许可证。详见 [LICENSE-MIT](LICENSE-MIT) 与 [LICENSE-APACHE](LICENSE-APACHE)。
