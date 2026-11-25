import {expect, test} from '@playwright/test';
import {deployment} from '../scripts/deployment.mjs';

const {base} = deployment();
const homeEn = base;
const homeZh = `${base}zh/`;
const docsEn = `${base}docs/`;
const docsZh = `${base}zh/docs/`;
const overviewEn = `${base}start/overview/`;
const overviewZh = `${base}zh/start/overview/`;

test('English homepage presents compiler, workbench, and runtime preview', async ({page}) => {
  await page.goto(homeEn);
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  await expect(page.locator('h1')).toContainText('Model in YAML');
  await expect(page.locator('#compiler')).toContainText('Define the app in YAML');
  await expect(page.locator('#runtime')).toContainText('Generated UI for common tasks');
  await expect(page.locator('.app-preview')).toBeVisible();

  const appTab = page.getByRole('tab', {name: 'appstruct.yaml'});
  await appTab.click();
  await expect(appTab).toHaveAttribute('aria-selected', 'true');
  await expect(page.locator('[data-workbench-panel="app"]')).toBeVisible();
  await expect(page.locator('[data-workbench-panel="work"]')).toBeHidden();

  const impactTab = page.getByRole('tab', {name: 'impact + preview'});
  await impactTab.click();
  await expect(page.locator('[data-workbench-panel="impact"]')).toBeVisible();

  const mcpTab = page.getByRole('tab', {name: 'AI Agent (MCP)'});
  await mcpTab.click();
  await expect(page.locator('[data-cli-cmd="mcp"]')).toBeVisible();
});

test('Chinese homepage renders dedicated Chinese HTML page and switches cleanly', async ({page}) => {
  await page.goto(homeZh);
  await expect(page.locator('html')).toHaveAttribute('lang', 'zh-CN');
  await expect(page.locator('h1')).toContainText('用 YAML 定义系统');
  await expect(page.locator('#compiler')).toContainText('一次建模，跨全栈确定性生成');

  await page.getByRole('link', {name: 'Switch to English'}).click();
  await expect(page).toHaveURL(homeEn);
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');

  await page.getByRole('link', {name: 'Switch to Chinese'}).click();
  await expect(page).toHaveURL(homeZh);
  await expect(page.locator('html')).toHaveAttribute('lang', 'zh-CN');
});

test('navigation opens the documentation catalog', async ({page}, info) => {
  await page.goto(homeEn);
  if (info.project.name === 'mobile') {
    await page.getByRole('button', {name: 'Open menu'}).click();
    await page.getByRole('navigation', {name: 'Mobile navigation'}).getByRole('link', {name: 'Docs'}).click();
  } else {
    await page.getByRole('navigation', {name: 'Primary navigation'}).getByRole('link', {name: 'Docs'}).click();
  }
  await expect(page).toHaveURL(docsEn);
  await expect(page.getByRole('heading', {level: 1, name: 'Guides and technical reference'})).toBeVisible();
});

test('documentation renders source, contents, and adjacent pages', async ({page}, info) => {
  await page.goto(overviewEn);
  await expect(page.getByRole('heading', {level: 1, name: 'Overview and quick start'})).toBeVisible();
  await expect(page.locator('.prose')).toContainText('AppStruct compiles a declarative, multi-file YAML App Spec');
  await expect(page.getByRole('link', {name: /View source/})).toBeVisible();
  if (info.project.name !== 'mobile') {
    await expect(page.getByRole('navigation', {name: 'On this page'}).first()).toBeVisible();
    await expect(page.locator('.prose h2 .heading-anchor').first()).toBeAttached();
  } else {
    await expect(page.locator('.docs-nav-panel summary')).toBeVisible();
    await expect(page.getByRole('navigation', {name: 'Documentation'}).getByRole('link', {name: 'Installation'})).toBeHidden();
    await page.locator('.docs-nav-panel summary').click();
    await expect(page.getByRole('navigation', {name: 'Documentation'}).getByRole('link', {name: 'Installation'})).toBeVisible();
  }
  await expect(page.getByRole('navigation', {name: 'Adjacent pages'}).getByRole('link', {name: /Installation/})).toBeVisible();
});

test('documentation header stays available while reading', async ({page}) => {
  await page.goto(overviewEn);
  await page.evaluate(() => window.scrollTo(0, 1400));
  await expect(page.locator('#site-header')).not.toHaveClass(/is-hidden/);
  await expect(page.getByRole('button', {name: 'Search'})).toBeVisible();
});

test('search finds documentation by title', async ({page}) => {
  await page.goto(homeEn);
  await page.getByRole('button', {name: 'Search'}).click();
  const dialog = page.getByRole('dialog', {name: 'Search docs'});
  await dialog.getByRole('searchbox', {name: 'Search docs'}).fill('Migration lint');
  await expect(dialog.locator('a[href*="/delivery/migration-lint/"]')).toBeVisible();
});

test('theme toggle persists preference across reload', async ({page}) => {
  await page.goto(homeEn);
  const root = page.locator('html');
  const before = await root.getAttribute('data-theme');
  await page.getByRole('button', {name: 'Toggle theme'}).click();
  const after = await root.getAttribute('data-theme');
  expect(after).not.toBe(before);
  await page.reload();
  await expect(root).toHaveAttribute('data-theme', after || '');
});

test('Chinese documentation renders dedicated ZH pages', async ({page}) => {
  await page.goto(overviewZh);
  await expect(page.locator('html')).toHaveAttribute('lang', 'zh-CN');
  await expect(page.getByRole('heading', {level: 1, name: '概览与快速开始'})).toBeVisible();
  await expect(page.locator('.prose')).toContainText('AppStruct 将多文件声明式 YAML App Spec 确定性编译为类型化中间表示（IR）');
  await page.goto(`${base}zh/start/installation/`);
  await expect(page.getByRole('heading', {level: 1, name: '安装'})).toBeVisible();
  await expect(page.locator('.prose')).toContainText('AppStruct 可以从源码检出安装。');
});

test('layouts do not overflow horizontally', async ({page}) => {
  for (const url of [homeEn, homeZh, docsEn, docsZh, overviewEn, overviewZh]) {
    await page.goto(url);
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
    expect(overflow).toBeLessThanOrEqual(1);
  }
});
