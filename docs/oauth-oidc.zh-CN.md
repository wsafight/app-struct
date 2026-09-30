# OAuth/OIDC 登录

启用 Auth 的项目可以通过以下配置打开生成的 OIDC 授权码流程：

```yaml
modules:
  auth:
    providers: [google, github]
```

`oauth: true` 仍是通用 `oidc` Provider 的快捷写法。Provider ID 稳定，并且可以同时启用多个；
生成界面只显示已启用的 Provider，编译器会拒绝未知 ID。Provider 凭据只通过环境变量配置：

对象式条目可以在保留配置的情况下禁用 Provider，也可以禁止创建新的本地用户，同时保留已绑定
账户的登录能力：

```yaml
providers:
  - id: google
    type: oidc
    enabled: true
    allow_signup: false
```

Google 与通用 OIDC 必须使用 `type: oidc`，GitHub 必须使用 `type: oauth`。重复 ID 或不兼容的
协议类型会在编译期报错。

- `APPSTRUCT_OIDC_AUTHORIZATION_URL`
- `APPSTRUCT_OIDC_TOKEN_URL`
- `APPSTRUCT_OIDC_USERINFO_URL`
- `APPSTRUCT_OIDC_CLIENT_ID`
- `APPSTRUCT_OIDC_CLIENT_SECRET`
- `APPSTRUCT_OIDC_REDIRECT_URI`

Google 使用官方 OIDC 地址，需要：

- `APPSTRUCT_GOOGLE_CLIENT_ID`
- `APPSTRUCT_GOOGLE_CLIENT_SECRET`
- `APPSTRUCT_GOOGLE_REDIRECT_URI`

GitHub 使用 OAuth API，需要：

- `APPSTRUCT_GITHUB_CLIENT_ID`
- `APPSTRUCT_GITHUB_CLIENT_SECRET`
- `APPSTRUCT_GITHUB_REDIRECT_URI`

需要在 Provider 侧同时登记 `/api/auth/oauth/<provider>/callback` 与
`/api/auth/oauth/<provider>/link/callback`。`*_REDIRECT_URI` 保存普通回调地址；AppStruct 会替换
末尾的 `/callback` 推导绑定回调，并在授权与 token exchange 中使用同一个地址。

`GET /api/auth/oauth/<provider>/start` 创建短时 HttpOnly state cookie，并重定向到 Provider。回调会校验该 state、交换 code、获取 Provider profile，然后创建或复用本地用户。Provider subject 映射存储在 `_appstruct_auth_oauth_accounts` 中；access 和 refresh token 永不持久化。通用 OIDC 和 Google 的 userinfo 必须同时包含 `email` 与 `email_verified`；GitHub 使用 `/user/emails`，只接受 primary 且 verified 的邮箱。

首次已验证的 OIDC 登录可以把相同规范化邮箱的现有本地用户关联起来。只有在账户和邮箱验证策略可信时才启用该 Provider；之后的登录使用已存储的 Provider subject 映射，而不再按邮箱匹配。

已登录用户可以在生成的账户界面中查看、绑定和解绑 Provider。绑定同时校验当前会话、短期 state
和用户 cookie；解绑最后一种可用登录方式会被拒绝。

启用 OAuth 后，生成的登录页会显示 SSO 按钮。Provider 失败返回 `502 OAUTH_PROVIDER_ERROR`；缺少环境配置返回 `500 OAUTH_CONFIGURATION_ERROR`。

使用 `appstruct capabilities --format json` 可以查看当前版本的 Auth/Billing Provider 支持矩阵。
