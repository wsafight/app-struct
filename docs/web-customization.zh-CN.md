# 应用品牌与主题

三个官方模板都提供用户拥有的 `app/web/setup.ts`。`generate` 和 `update` 保留该文件；生成 Web 的入口会在 React 启动前加载它。已有项目可以手动创建同一路径，无需修改 `generated/`。

```ts
import { configureUI } from "@appstruct/ui";

configureUI({
  locale: "zh-CN",
  timeZone: "Asia/Shanghai",
  title: "订单工作台",
  logo: { src: "https://assets.example.com/logo.svg", alt: "公司" },
  colorScheme: "system",
  colors: { accent: "#047857", "accent-hover": "#065f46" },
  messages: {
    workspace: "工作区",
    resources: "业务数据",
    addRecord: "新增{resource}",
    editRecord: "编辑{resource}",
    save: "保存",
    cancel: "取消",
  },
});
```

`colorScheme` 支持 `light`、`dark` 和跟随操作系统的 `system`。`colors` 使用类型化的 CSS 色彩变量名；覆盖值同时适用于明暗主题。Logo URL 必须是浏览器可访问的应用资产地址。需要打包本地图片时，可在 setup 中导入图片并将导入结果传给 `src`。

`locale` 支持 `en`（默认）和 `zh-CN`。标准列表、表单、认证、校验、CSV、编辑冲突及管理模块提供中文文案。`messages` 的类型化键覆盖常用文案，`translations` 可按标准英文源文案覆盖其他文本；缺项回退到内置语言，再回退英文。文案按普通文本呈现。此配置在启动时生效，尚无运行时语言切换。

资源名称使用 Spec 的实体 `label`；业务值、枚举、用户评论和自定义错误不自动翻译。字段显示名可通过 `fieldLabels: { "app::Order": { starts_at: "开始时间", amount: "金额" } }` 配置；自定义页面可使用 `t`、`uiText` 与同一格式化函数。

`timeZone` 控制只读时间戳；默认浏览器时区，非法时区在 setup 时拒绝。纯日期不跨时区移动；数字、精确小数和金额按 `locale` 显示并保留精度。`datetime-local` 始终按浏览器时区输入，表单显示该时区，codec 继续拒绝无效日历与夏令时缺口。API、CSV 和 UTC 报表分组不受显示配置影响。

原生日期时间控件最多显示和编辑毫秒；编辑已有记录时，未改动的时间保留原始微秒与 UTC 偏移。新输入或改动的时间按控件精度提交。

配置完成后运行：

```bash
cd generated/web
pnpm run typecheck
pnpm run lint
pnpm test
pnpm run build
```

在应用目标浏览器检查 Logo、长标题、明暗主题、移动布局，以及键盘焦点和颜色对比。可配置的颜色不会自动保证可访问性。
