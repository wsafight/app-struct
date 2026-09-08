# OAuth/OIDC 登录

启用 Auth 的项目可以通过以下配置打开生成的 OIDC 授权码流程：

```yaml
modules:
  auth:
    oauth: true
```

Provider 只通过环境变量配置：

- `APPSTRUCT_OIDC_AUTHORIZATION_URL`
- `APPSTRUCT_OIDC_TOKEN_URL`
- `APPSTRUCT_OIDC_USERINFO_URL`
- `APPSTRUCT_OIDC_CLIENT_ID`
- `APPSTRUCT_OIDC_CLIENT_SECRET`
- `APPSTRUCT_OIDC_REDIRECT_URI`

`GET /api/auth/oauth/oidc/start` 创建短时 HttpOnly state cookie，并重定向到 Provider。回调会校验该 state、交换 code、获取 OpenID userinfo，然后创建或复用本地用户。Provider subject 映射存储在 `_appstruct_auth_oauth_accounts` 中；access 和 refresh token 永不持久化。只有当 Provider 返回标准布尔声明 `"email_verified": true` 时，新 OIDC 用户才会被标记为已验证邮箱。缺失、false 或非布尔的 `email_verified` 声明会拒绝登录。请配置 Provider，使其 userinfo 响应同时包含 `email` 和 `email_verified`。

首次已验证的 OIDC 登录可以把相同规范化邮箱的现有本地用户关联起来。只有在账户和邮箱验证策略可信时才启用该 Provider；之后的登录使用已存储的 Provider subject 映射，而不再按邮箱匹配。

启用 OAuth 后，生成的登录页会显示 SSO 按钮。Provider 失败返回 `502 OAUTH_PROVIDER_ERROR`；缺少环境配置返回 `500 OAUTH_CONFIGURATION_ERROR`。
