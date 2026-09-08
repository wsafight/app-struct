# 组织邀请

启用 Tenant 模块后，组织所有者可以邀请已有 Auth 用户加入当前组织。邀请由 `X-AppStruct-Tenant` 头限定范围，并通过生成的 API 和 React 租户页面提供。

## API

- `GET /api/tenant/invitations` 列出当前组织的邀请。
- `POST /api/tenant/invitations` 创建有效期七天的邀请。请求体接受邮箱和可选 `role`（当前为 `member`）。向同一地址再次发送会替换较旧的待处理邀请。配置了 Auth 邮件发送器时，生成运行时会发送链接。
- `DELETE /api/tenant/invitations/{id}` 撤销待处理邀请。
- `POST /api/tenant/invitations/{token}/accept` 为规范化邮箱与邀请匹配的已认证用户接受链接。成员插入是幂等的，Token 只能消耗一次。

所有管理操作都需要已认证的组织所有者、CSRF 校验和有效的租户成员关系。邀请 Token 是随机不透明值；`_appstruct_tenant_invitations` 中只存储 SHA-256 哈希。过期、已撤销、已接受或邮箱不匹配的链接会返回错误，且不泄露组织成员关系。

Web 运行时从租户切换器暴露 Organization 页面。所有者可以发送和撤销邀请，并查看待处理或已接受状态。邀请链接打开接受页面；接受后会在本地选中新组织。
