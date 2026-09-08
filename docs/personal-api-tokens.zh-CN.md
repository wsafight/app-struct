# 个人 API Token

启用 Auth 的应用会为脚本和自动化暴露个人 Bearer Token。创建 Token 时需要显示名称，以及可选的 1 到 3650 天有效期：

- `GET /api/auth/tokens` 列出当前用户的 Token 元数据。
- `POST /api/auth/tokens` 创建 Token，并仅返回一次明文。
- `DELETE /api/auth/tokens/{id}` 撤销当前用户的一个 Token。
- API 请求可以使用 `Authorization: Bearer <token>` 认证。

生成数据库只存储 SHA-256 Token 哈希。过期和已撤销的 Token 会被拒绝，成功的 Bearer 请求会更新 `last_used_at`。Token 管理使用常规会话和 CSRF 保护；Bearer 认证的请求不需要会话 cookie。Web 运行时增加 API Token 页面，提供一次性复制创建和撤销控件，OpenAPI 同时发布 cookie 和 bearer 安全方案。
