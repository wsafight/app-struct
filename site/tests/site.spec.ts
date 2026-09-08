import {expect, test} from '@playwright/test';
import {deployment} from '../scripts/deployment.mjs';

const {base} = deployment();
const home = base;
const docs = `${base}docs/`;
const overview = `${base}start/overview/`;

test('homepage presents the compiler and generated runtime', async ({page}) => {
  await page.goto(home);
  await expect(page.getByRole('heading', {level: 1, name: 'AppStruct'})).toBeVisible();
  await expect(page.locator('#compiler')).toContainText('Describe intent');
  await expect(page.locator('#runtime')).toContainText('Useful on the first run');
  await expect(page.locator('.app-preview')).toBeVisible();
});

test('navigation opens the documentation catalog', async ({page}, info) => {
  await page.goto(home);
  if (info.project.name === 'mobile') {
    await page.getByRole('button', {name: 'Open menu'}).click();
    await page.getByRole('navigation', {name: 'Mobile navigation'}).getByRole('link', {name: 'Docs'}).click();
  } else {
    await page.getByRole('navigation', {name: 'Primary navigation'}).getByRole('link', {name: 'Docs'}).click();
  }
  await expect(page).toHaveURL(docs);
  await expect(page.getByRole('heading', {level: 1, name: 'Build from one source of truth'})).toBeVisible();
});

test('documentation renders source, contents, and adjacent pages', async ({page}, info) => {
  await page.goto(overview);
  await expect(page.getByRole('heading', {level: 1, name: 'Overview and quick start'})).toBeVisible();
  await expect(page.getByText('AppStruct is a configuration-driven Rust full-stack application generator.')).toBeVisible();
  await expect(page.getByRole('link', {name: /View source/})).toBeVisible();
  if (info.project.name !== 'mobile') await expect(page.getByRole('navigation', {name: 'On this page'}).first()).toBeVisible();
  await expect(page.getByRole('navigation', {name: 'Adjacent pages'}).getByRole('link', {name: /Installation/})).toBeVisible();
});

test('search finds documentation by title', async ({page}) => {
  await page.goto(home);
  await page.getByRole('button', {name: 'Search'}).click();
  const dialog = page.getByRole('dialog', {name: 'Search docs'});
  await dialog.getByRole('searchbox', {name: 'Search docs'}).fill('Migration lint');
  await expect(dialog.getByRole('link', {name: /Migration lint/})).toBeVisible();
});

test('theme and language controls persist preferences', async ({page}) => {
  await page.goto(home);
  const root = page.locator('html');
  const before = await root.getAttribute('data-theme');
  await page.getByRole('button', {name: 'Toggle theme'}).click();
  await expect(root).not.toHaveAttribute('data-theme', before || '');
  await page.getByRole('button', {name: 'Switch to Chinese'}).click();
  await expect(root).toHaveAttribute('data-lang', 'zh');
  await expect(page.getByText('把一份应用规格，编译成类型安全的 Rust 全栈系统。')).toBeVisible();
  await page.reload();
  await expect(root).toHaveAttribute('data-lang', 'zh');
});

test('Chinese documentation renders translated body copy', async ({page}) => {
  await page.goto(`${overview}?lang=zh`);
  await expect(page.locator('html')).toHaveAttribute('data-lang', 'zh');
  await expect(page.getByRole('heading', {level: 1, name: '概览与快速开始'})).toBeVisible();
  await expect(page.locator('.prose.i18n-zh')).toContainText('AppStruct 是一个由配置驱动的 Rust 全栈应用生成器。');
  await page.goto(`${base}start/installation/?lang=zh`);
  await expect(page.getByRole('heading', {level: 1, name: '安装'})).toBeVisible();
  await expect(page.locator('.prose.i18n-zh')).toContainText('AppStruct 可以从源码检出安装。');
});

test('layouts do not overflow horizontally', async ({page}) => {
  for (const url of [home, docs, overview]) {
    await page.goto(url);
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
    expect(overflow).toBeLessThanOrEqual(1);
  }
});
