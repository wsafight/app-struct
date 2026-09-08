# AppStruct 文档站点

文档站点是一个静态 Astro 应用。页面目录在 `scripts/catalog.mjs`；`scripts/prepare.mjs` 会把仓库中的 Markdown 导入 Astro content collections，并把仓库相对链接改写为站点链接。英文页面使用原文 Markdown，中文页面使用对应的 `*.zh-CN.md` 译文。

```bash
npm ci
npm run dev
```

开发服务器监听 `http://127.0.0.1:4321`。生产校验会运行类型检查、静态构建、Pagefind 索引和内部链接检查：

```bash
npm run check
npm run build
npx playwright install chromium
npm test
```

`DOCS_SITE` 覆盖规范站点 origin，`DOCS_BASE` 覆盖部署路径。未设置这些变量时，GitHub Actions 会根据 `GITHUB_REPOSITORY` 推导 GitHub Pages 路径。
