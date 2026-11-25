# AppStruct 产品概览

> **单一事实来源**：快速上手与完整能力说明见 [README.zh-CN.md](README.zh-CN.md)，详细专题文档见 `docs/`。

## 定位（Open-Core）
- **核心切入点**：面向 AI Coding Agent 的确定性全栈编译器（内置 `appstruct mcp`、Draft 2020-12 JSON Schema、`appstruct check --json`，编译生成安全的 Rust + React 全栈应用）。
- **高转化场景**：存量 PostgreSQL 数据库 3 分钟生成安全合规的管理后台与 SaaS 服务（`appstruct db pull`，单二进制高性能、源码完全可控、无按席位锁定）。

## 分层能力
- **OSS 开源版（MIT / Apache-2.0）**：完整编译器与 Typed IR、PostgreSQL 迁移、`db pull`（表/视图/物化视图/复合主键代理键桥接）、基础 CRUD/RBAC/认证、`admin`/`dashboard` 模板、`appstruct dev`、`appstruct mcp`、声明式 HTTP Command/Query。
- **Pro 商业套件**：`appstruct/saas` 预设（计费/点数、MFA、团队、通知、文件字段）、一键生产部署适配器（Fly.io / Railway / Coolify / Caddy 自动 HTTPS）与垂直行业 Starter（`starters/b2b-crm-ticketing`、`starters/ai-workbench`）。
- **Enterprise 企业版**：可选 PostgreSQL 行级安全策略生成（`modules.tenant.rls`）、按角色动态字段脱敏（API 与 CSV 同步生效）、GDPR 数据导出与彻底删除、OIDC/OAuth + MFA、只读计算字段、跨资源经营仪表盘（`/api/dashboard`）与可视化权限矩阵审查台（`appstruct access matrix`）。
