# AppStruct site

The documentation site is a static Astro application. Its page catalog lives in
`scripts/catalog.mjs`; `scripts/prepare.mjs` imports the repository Markdown into Astro content
collections and rewrites repository-relative links for the deployed site. English pages use the
source Markdown; Chinese pages use the matching `*.zh-CN.md` translation.

```bash
npm ci
npm run dev
```

The development server listens on `http://127.0.0.1:4321`. Type checks, Playwright tests, and
link checks stay local. GitHub Actions only builds the static site and deploys GitHub Pages:

```bash
npm run check
npm run build
npx playwright install chromium
npm test
```

`DOCS_SITE` overrides the canonical origin and `DOCS_BASE` overrides the deployment path. Without
those variables, the Pages workflow derives the project path from `GITHUB_REPOSITORY`.

中文说明见 [README.zh-CN.md](README.zh-CN.md)。
