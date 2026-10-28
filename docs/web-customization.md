# Application branding and theme

All three official templates provide the user-owned `app/web/setup.ts`. Generation and update preserve it. The generated Web entry loads it before React starts. Existing projects can create this path without editing generated files.

```ts
import { configureUI } from "@appstruct/ui";

configureUI({
  locale: "zh-CN",
  timeZone: "Asia/Shanghai",
  title: "Order desk",
  logo: { src: "https://assets.example.com/logo.svg", alt: "Company" },
  colorScheme: "system",
  colors: { accent: "#047857", "accent-hover": "#065f46" },
  messages: { addRecord: "New {resource}", save: "Save changes" },
});
```

The color scheme accepts `light`, `dark`, or `system`; system follows OS changes. Typed palette overrides apply to both themes. Use a browser-accessible logo URL, or import a local image in the setup module and pass its bundled URL as `src`.

`locale` supports `en` (default) and `zh-CN`. Standard lists, forms, authentication, validation, CSV, conflict comparison and management pages include Chinese copy. Typed `messages` overrides common keys; `translations` overrides other standard English source messages, falling back to the built-in locale and then English. Configuration applies at startup; runtime language switching is not provided.

Entity names use the Spec label. Business values, enums, comments and custom application errors are not automatically translated. Configure field display names with `fieldLabels: { "app::Order": { starts_at: "开始时间", amount: "金额" } }`. Custom pages can reuse `t`, `uiText` and the shared formatting helpers.

`timeZone` controls read-only timestamps and defaults to the browser zone; invalid zones fail during setup. Calendar-only dates never shift days. Locale-aware numbers and money keep exact decimal precision. `datetime-local` input uses the browser zone, which the form displays; codecs still reject invalid calendars and DST gaps. API/CSV values and UTC report buckets are unchanged by display settings.

Native datetime controls display and edit at most milliseconds. Unchanged timestamps on existing records retain their original microseconds and UTC offset; new or edited timestamps use the control's precision.

Run `pnpm run typecheck`, `pnpm run lint`, `pnpm test` and `pnpm run build` from `generated/web`. Check the logo, long titles, both themes, mobile layout, focus and contrast in target browsers. Palette overrides do not guarantee accessibility.
