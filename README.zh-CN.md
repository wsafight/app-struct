# AppStruct

[English](README.md) | [简体中文](README.zh-CN.md)

AppStruct 是一个由配置驱动的 Rust 全栈应用生成器。它把多文件 YAML App Spec 编译为类型化
中间表示（IR）、PostgreSQL 迁移、Axum/SeaORM 后端、OpenAPI、TypeScript 客户端以及
React/Vite Web 应用。

当前仓库处于技术预览阶段。现在需要从源码构建 CLI，尚未发布 crates.io 包或独立安装器。
M0-M6 已完成，包含生产构建、协调式开发服务器、Tenant/Audit/Mail/Jobs/File 模块、锁定的
`appstruct/saas@1` 预设、事务化项目更新，以及可运行的 SaaS 模板和示例。SaaS 预设提供
Admin 运维概览和受保护的 Jobs 重试/重放操作；Billing 不在预设 v1 的范围内。

## 环境要求

| 依赖 | 版本 | 用途 |
| --- | --- | --- |
| Rust | 1.98.0，带 rustfmt 和 Clippy | CLI 和生成的后端 |
| PostgreSQL | 推荐 17 | 迁移和生成的 API |
| Node.js | 推荐 24 | 生成的 Web 应用 |
| pnpm | 9.12.3 | 按锁文件安装 Web 依赖 |
| Docker Compose | 当前版本，可选 | 仅用于 `database.dev.mode: managed` |

仓库根目录和生成项目都会固定 Rust 版本。Web 模板也会提交 pnpm 锁文件，确保不同机器上的
安装和构建可复现。

## 安装 CLI

在仓库根目录执行：

```bash
rustup toolchain install 1.98.0 --component clippy,rustfmt
cargo build --release --locked -p appstruct-cli
./target/release/appstruct --version
```

可以直接使用 `target/release/appstruct`，也可以复制到已有的 `PATH` 目录：

```bash
install -d "$HOME/.local/bin"
install -m 0755 target/release/appstruct "$HOME/.local/bin/appstruct"
appstruct --version
```

从源码构建时请保留 `--locked`，这样 Cargo 会使用仓库中提交的 `Cargo.lock`。当前源码布局
不把 `cargo install --path crates/appstruct-cli` 作为可复现安装方式。

## 快速开始

### 使用已有 PostgreSQL

`minimal` 模板连接外部数据库：

```bash
appstruct new notes --template minimal
cd notes
cp .env.example .env
```

编辑 `.env` 中的 `DATABASE_URL`，然后执行：

```bash
export DATABASE_URL=postgresql://user:password@127.0.0.1:5432/notes
appstruct migrate dev --accept
appstruct doctor
appstruct dev
```

外部数据库默认使用 `database.dev.migration: unmanaged`，因此首次启动前需要显式运行迁移。
`appstruct dev` 随后会生成并构建后端、安装锁定的 Web 依赖，并启动 API 和 Vite。默认地址为
`http://127.0.0.1:3000` 和 `http://127.0.0.1:5173`，也可以通过 `--api-port` 与 `--web-port`
修改。

### 使用托管 PostgreSQL

`dashboard` 模板包含 `compose.yaml`，默认由 AppStruct 管理 PostgreSQL：

```bash
appstruct new project-hub --template dashboard
cd project-hub
appstruct doctor
appstruct dev
```

托管模式只启动 Compose 中的 `postgres` 服务。当前开发会话启动的服务会在 Ctrl-C 时停止，
命名卷会保留；原本已运行的服务不会被停止。托管模式默认使用
`database.dev.migration: prompt`：只有检测到待处理迁移时才询问。迁移确认后才会创建或应用
迁移。

### 使用 SaaS 预设

```bash
appstruct new saas-demo --template saas
cd saas-demo
appstruct preset show
appstruct doctor
appstruct dev
```

该模板锁定 `appstruct/saas@1`，包含 Auth、Tenant、Audit、Mail、Jobs、File 等模块。注册后可
创建组织并使用 Project、Task 资源；它们默认启用租户隔离和审计。开发环境使用 capture 邮件
和本地文件目录 `.appstruct/files`，Jobs/Outbox 使用 PostgreSQL。

首次注册的用户是 `member` 角色。需要管理员权限时，请从可信主机执行一次：

```bash
appstruct auth bootstrap-admin --email admin@example.com
```

`appstruct.lock` 必须提交，其中包含预设摘要和精确的模块版本。使用
`appstruct preset show --expanded` 查看应用覆盖后的最终配置。

## 开发迁移策略

在项目的 `appstruct.yaml` 中显式声明开发数据库归属和迁移行为：

```yaml
database:
  provider: postgres
  dev:
    mode: managed
    migration: prompt # auto | prompt | never | unmanaged
```

四种策略的含义如下：

- `auto`：创建并应用安全的在线迁移。
- `prompt`：仅在存在待处理工作时询问，然后按确认结果继续。
- `never`：只读检查兼容性；发现数据库过期时阻止启动。
- `unmanaged`：启动前跳过 AppStruct 的迁移和 schema 检查，完全由操作者管理。

`mode: managed` 负责启动本地 Compose 数据库，`mode: external` 不会启动或停止 PostgreSQL。
生产后端启动永远不会自动执行迁移；发布流程中请单独运行 `migrate status` 和明确的
`migrate apply`。

## 生成能力

Web 运行时采用固定且现代的 React 19 + TypeScript + Vite 基线，并集成：

- TanStack Query：服务端数据获取和缓存。
- TanStack Router：类型安全路由和认证/租户路由保护。
- TanStack Table：资源列表、排序、筛选和分页。
- TanStack Form + Zod：表单状态和 schema 校验。

生成的资源 API 支持偏移分页、主键游标分页、筛选、排序、关系过滤、聚合和分组查询；所有
查询都会继续应用 actor、资源和租户权限。字段可以独立声明读写规则，未授权字段不会出现在
响应中，未授权提交字段会被后端拒绝。

其他可用能力包括：

- PostgreSQL `db pull`：只读生成现有表结构的 Spec 草稿。
- 复合/部分索引和可审查的 seed 数据。
- Auth、Tenant、Audit、Mail、Jobs、File 模块。
- 批量操作、CSV 导入导出、保存的列表视图、软删除/恢复、组织邀请、邮箱验证、OAuth/OIDC
  和个人 API Token。
- 生成 Axum/SeaORM 后端、OpenAPI 文档和 TypeScript 客户端。

## 常用命令

```text
appstruct new <name> --template minimal|dashboard|saas
appstruct schema
appstruct check [--deny-warnings] [--format text|json]
appstruct generate [--check]
appstruct migrate plan|dev|lint|apply|status
appstruct dev [--api-port <port>] [--web-port <port>]
appstruct build
appstruct doctor [--format text|json]
appstruct db pull [--schema <name>] [--output <project-relative-path>]
appstruct auth bootstrap-admin --email <address>
appstruct preset show [--expanded]
appstruct update
```

`migrate plan` 只读；`migrate dev --accept` 只创建并可应用非破坏性在线迁移。生产发布应先
运行 `migrate status`，再运行明确的 `migrate apply`。`migrate lint` 会报告破坏性、锁表和
不安全非空变更，可在 CI 中配合 `--deny-warnings` 使用。

## 文档索引

- [安装](docs/installation.md)
- [部署](docs/deployment.md)
- [升级](docs/upgrading.md)
- [数据查询](docs/data-querying.md)
- [Schema 索引](docs/schema-indexes.md)
- [Seed 数据](docs/seeding.md)
- [迁移检查](docs/migration-lint.md)
- [发布流程](docs/releasing.md)
- [产品路线图](docs/next-product-roadmap.md)
- [产品需求](PRODUCT.md)
- [技术设计](TECHNICAL_DESIGN.md)

`references/` 是仅供本地研究使用的外部资料目录，已通过 `.gitignore` 排除，不属于产品提交。

## 质量检查

使用仓库固定工具链运行：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

需要 PostgreSQL 的浏览器测试使用独立测试数据库，并通过对应的 `scripts/run-*-e2e.sh` 脚本
执行。生成项目还应运行 `pnpm install --frozen-lockfile`、`pnpm run format:check`、
`pnpm run typecheck` 和 `pnpm run build`。

提交前检查工作区：

```bash
git status --short --branch
git diff --check
```

不要提交真实 `.env`、密码、API 密钥、私钥/证书、`node_modules/`、`target/`、`references/`、
Playwright 报告或 `test-results/`。`.env.example` 中的占位配置可以提交。

## 当前边界

AppStruct 仍是技术预览。当前发布流程需要维护者手动配置 GitHub 远端、执行质量门，并在
生产发布中单独审核和应用迁移。Billing、定时任务、签名 Webhook、部署适配器和可视化编辑器
属于后续路线图，不应被视为当前预设 v1 的稳定承诺。

## 许可证

Workspace 中的 crate 使用 MIT OR Apache-2.0 双许可证。详见 `LICENSE-MIT` 与 `LICENSE-APACHE`。
