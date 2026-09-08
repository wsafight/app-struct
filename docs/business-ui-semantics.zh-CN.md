# 业务 UI 语义

> Status: money v1 accepted; quantity deferred
> Date: 2026-09-05

## 目的

业务 UI 语义描述现有标量字段如何展示和编辑。它们不添加数据库类型、不更改 REST 载荷，也不削弱字段和资源授权。第一份契约有意限于同一实体上的金额字段和货币字段。

Operations Demo 在 `SupplierOffer` 和 `OrderLine` 中都包含这种形状，足以证明一份可复用的展示契约。其数量字段通过 Product 关系派生单位，因此数量会推迟到关系展示数据有确定的加载和一致性契约之后。

## Money v1

```yaml
unit_price:
  type: decimal
  required: true
  ui:
    semantic: money
    currency_field: currency
    fraction_digits: 2
currency:
  type: enum
  required: true
  values: [CNY, USD, EUR]
```

仅当以下所有不变量都成立时，`ui.semantic: money` 才有效：

- 被标注的金额是非 generated 的 Decimal 字段；
- `currency_field` 命名一个同级 Enum 字段；
- 每个货币值都是三个大写字母的 ISO 风格代码；
- 金额和货币要么都必填，要么都可选；
- 金额和货币具有相同的字段级读和写访问；
- 货币字段不是自定义组件、另一个语义字段，或第二个 money 字段的伴生字段；
- `fraction_digits` 是 0 到 6 的整数；
- `ui.component` 和 `ui.semantic` 互斥。

生成的 Web 运行时渲染一个组合的金额/货币控件。列表和详情值使用伴生货币代码、固定小数位数、区域感知分隔符以及表格数字对齐。伴生货币仍可用于过滤和显式列选择，但会从默认列和详情网格中省略，因为金额值已经包含它。v1 中语义字段不可行内编辑，因为金额和货币必须一起保存。

`fraction_digits` 只控制 Web 输入步进和展示舍入。它不是存储精度或 API 校验规则。需要精确会计精度的应用必须在领域校验中强制执行。省略时默认为 2。

如果自定义客户端收到它无法满足的元数据，必须回退到底层 Decimal 和 Enum 字段。API 继续独立授权和校验这两个字段。

## 推迟的数量契约

Operations Demo 把 Product 单位与 Inventory 和 OrderLine 数量分开存储。因此有用的数量控件需要关系标签展开、加载和不可用状态、相关 Product 的授权，以及记录已存在后单位变更的规则。这些契约不是 money v1 的一部分。在至少一个真实示例需要该数据形状之前，不应添加同记录的 `quantity + unit` 语法。

## 非目标

- 货币转换、汇率、税、合计或会计规则；
- PostgreSQL money 类型或新的 REST 标量；
- 从租户推断区域或货币；
- Value Objects 或 Workflow 输入中的语义字段；
- 行包含多种货币时感知聚合的格式化。
