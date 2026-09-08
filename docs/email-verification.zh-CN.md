# 邮箱验证

启用 Auth 的应用会在用户注册时签发邮箱验证链接，并为已登录用户提供重发端点。验证 Token 是随机不透明值，只以 SHA-256 哈希存储在 `_appstruct_auth_email_verifications` 中；每个 Token 24 小时后过期，且只能使用一次。

## API

- `POST /api/auth/email/request` 需要当前会话和 CSRF Token。对已验证账户幂等，否则会替换先前待处理的 Token。
- `POST /api/auth/email/verify` 接受 `{ "token": "..." }`，并在消耗 Token 的同一事务中标记匹配账户的 `email_verified_at`。
- Auth 响应包含 `email_verified`，客户端可以据此展示当前状态。

生成的 React 应用提供 `/verify-email?token=...`，以及请求新邮件的客户端方法。无效、过期或已使用的 Token 返回 `400 INVALID_EMAIL_VERIFICATION_TOKEN`。邮件投递使用现有 Auth Mail Sender，并在账户/Token 写入后尽力发送；部署应只在开发环境配置 capture，生产环境使用 SMTP。
