# AppStruct 技术架构

> **单一事实来源**：详细专题指南位于 `docs/`，当前格式契约版本见 [docs/compatibility.zh-CN.md](docs/compatibility.zh-CN.md)。

## 编译与生成流水线
1. **App Spec（YAML）**：通过 Draft 2020-12 JSON Schema（`crates/appstruct-compiler/schema/appstruct.schema.json`）做静态结构校验。
2. **编译器（`appstruct-compiler`）**：将根配置与领域文件降级为规范化 Typed IR（`appstruct-ir`），输出确定性诊断。
3. **数据库 Schema 与迁移（`appstruct-schema`、`appstruct-migrate`）**：生成 PostgreSQL DDL（含可选 `ENABLE/FORCE ROW LEVEL SECURITY` 租户隔离策略）、静态迁移风险检查，并支持内省存量 PostgreSQL 表、普通视图与物化视图。
4. **确定性代码生成（`appstruct-codegen`）**：输出 Axum + SQLx Rust 后端、强类型 TypeScript 客户端、React 前端与 OpenAPI 契约，并通过 SHA-256 所有权清单隔离手写扩展代码。
5. **CLI 与 MCP（`appstruct-cli`）**：提供 `init`/`new`、`check`、`build`、`dev`、`db pull`、`migrate`、`deploy`（Fly.io / Railway / Coolify / Caddy）、`access preview` / `access matrix`、`update`（Git 洁净检查 + 原子目录替换）以及标准 `mcp`（JSON-RPC 2.0 stdio）服务。

## 极简双轨扩展架构
- **进程外 HTTP 插件**：声明式 HTTP Command/Query（HMAC-SHA256 签名、超时与重试）与外部 HTTP 报表渲染器（`renderer: http`），让 Python/Node/Go/Rust 外部服务零门槛接入。
- **构建期组合模块**：`auth`、`tenant`、`billing`、`audit`、`file`、`jobs`、`mail`、`webhooks` 等全栈模块在编译期通过 Cargo 与 pnpm 原生生态按需拼装。
