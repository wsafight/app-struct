# 实体工作流

Workflow v1 把一个必填枚举字段变成服务端管理的状态机。编译器会把该字段从普通创建、更新、批量更新和 CSV 导入输入中移除；创建写入声明的初始状态，之后的变更通过转换端点进行。

```yaml
entities:
  Order:
    fields:
      status:
        type: enum
        required: true
        values: [draft, auditing, approved, rejected]
    workflow:
      field: status
      initial: draft
      transitions:
        submit:
          from: [draft, rejected]
          to: auditing
          access: { owner: owner }
        approve:
          from: [auditing]
          to: approved
          access: { role: auditor }
        reject:
          from: [auditing]
          to: rejected
          input: RejectOrderInput
          access: { role: auditor }
```

工作流字段必须是没有 `default`、`generated` 或字段级写访问的必填枚举。每个源、目标和初始状态都必须是枚举值。转换名称和边必须唯一，引用的输入值对象必须存在，并且每个枚举状态都必须能从初始状态到达。

## 生成契约

对于表为 `orders` 的实体，生成的后端暴露：

- `GET /api/orders/{id}/_transitions`，返回当前状态、修订以及当前操作者可见的转换。
- `POST /api/orders/{id}/_transitions/{action}`，执行一次转换。请求必须在 `If-Match` 中包含最新 ETag；JSON 体是配置的值对象，或对没有输入的转换为 `{}`。

生成的 TypeScript 资源客户端暴露 `transitions(id)` 和 `transition(id, action, input?)`。React 详情视图获取能力，并且只渲染后端返回的操作。这是便利，不是授权边界。

执行会应用租户和读范围，用 `FOR UPDATE` 加载行，检查修订和源状态，评估声明式访问，运行 `before_transition`，检查扩展 Policy，更新状态和修订，并运行 `after_transition`。写入和所有已启用的集成在同一事务中提交。带审计的实体会用输入 digest 记录 `workflow.<action>`；Activity 记录同一系统事件。Webhooks 使用 `<entity_event_prefix>.workflow.<action>`，Realtime 在提交后发布该事件。

稳定的工作流错误包括 `UNKNOWN_WORKFLOW_TRANSITION`（404）、`INVALID_WORKFLOW_STATE`（409）、`INVALID_WORKFLOW_INPUT`（422）、`PRECONDITION_REQUIRED`（428）和 `CONCURRENT_MODIFICATION`（412）。不可见记录返回常规的未找到响应。

## V1 边界

Workflow v1 支持每个实体一个受管理的工作流字段。它不实现 BPMN、任意脚本、请求幂等键、sagas 或分布式多聚合工作流。重复提交由当前状态和修订检查解决。对不是状态转换的操作使用普通 Commands。
