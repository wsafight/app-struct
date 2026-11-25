# AppStruct Product Overview

> **Single source of truth**: See [README.md](README.md) for the quickstart, positioning, and feature overview, and `docs/` for detailed guides.

## Positioning (Open-Core)
- **Primary hook**: Deterministic full-stack compiler for AI Coding Agents (`appstruct mcp`, JSON Schema Draft 2020-12, `appstruct check --json`, safe Rust + React generation).
- **Secondary hook**: Turn existing PostgreSQL databases into secure, auditable internal tools and SaaS backends in minutes (`appstruct db pull`, single binary, zero vendor lock-in).

## Tiers
- **OSS (MIT / Apache-2.0)**: Compiler, Typed IR, PostgreSQL migrations, `db pull` (tables, views, materialized views, composite-PK surrogate bridging), CRUD/RBAC/Auth, `admin`/`dashboard` starter, `appstruct dev`, `appstruct mcp`, declarative HTTP Commands/Queries.
- **Pro Kit**: `appstruct/saas` preset (billing/credits, MFA, teams, notifications, files), one-command production deploy adapters (Fly.io, Railway, Coolify, Caddy HTTPS), and vertical starters (`starters/b2b-crm-ticketing`, `starters/ai-workbench`).
- **Enterprise**: Optional PostgreSQL Row-Level Security (`modules.tenant.rls`), role-based dynamic PII field masking (API & CSV), GDPR account export/delete, OIDC/OAuth + MFA, read-only computed fields, cross-resource operational dashboards (`/api/dashboard`), and visual HTML access permission matrix (`appstruct access matrix`).
