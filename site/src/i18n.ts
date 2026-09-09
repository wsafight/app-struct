export type Lang = 'en' | 'zh';

export const ui = {
  en: {
    skip: 'Skip to main content', home: 'AppStruct home', primaryNav: 'Primary navigation',
    mobileNav: 'Mobile navigation', footerNav: 'Footer navigation', docsNav: 'Documentation',
    toc: 'On this page', pager: 'Adjacent pages', previous: 'Previous', next: 'Next',
    search: 'Search', searchDocs: 'Search docs', searchEmpty: 'No matching documentation.',
    theme: 'Toggle theme', menuOpen: 'Open menu', menuClose: 'Close menu', lang: 'Switch to Chinese',
    copy: 'Copy', copied: 'Copied', compiler: 'Compiler', runtime: 'Runtime', modules: 'Modules',
    delivery: 'Delivery', docs: 'Docs', browseDocs: 'Browse docs', linkHeading: 'Link to this section',
    quickStart: 'Quick start', github: 'GitHub',
  },
  zh: {
    skip: '跳到主要内容', home: 'AppStruct 首页', primaryNav: '主要导航', mobileNav: '移动端导航',
    footerNav: '页脚导航', docsNav: '文档目录', toc: '本页目录', pager: '相邻文档',
    previous: '上一篇', next: '下一篇', search: '搜索', searchDocs: '搜索文档',
    searchEmpty: '没有匹配的文档。', theme: '切换主题', menuOpen: '打开菜单', menuClose: '关闭菜单',
    lang: 'Switch to English', copy: '复制', copied: '已复制', compiler: '编译器', runtime: '运行时',
    modules: '模块', delivery: '交付', docs: '文档', browseDocs: '浏览文档', linkHeading: '链到本节',
    quickStart: '快速开始', github: 'GitHub',
  },
} as const;

export const githubRepo = 'https://github.com/wsafight/app-struct';

export const pagesMeta = {
  home: {
    title: {en: 'AppStruct - compile application specs into Rust systems', zh: 'AppStruct - 把应用规格编译成 Rust 全栈系统'},
    description: {
      en: 'AppStruct compiles a validated YAML App Spec into PostgreSQL migrations, an Axum backend, OpenAPI, a TypeScript client, and a React application.',
      zh: 'AppStruct 将经过校验的 YAML App Spec 编译为 PostgreSQL 迁移、Axum 后端、OpenAPI、TypeScript 客户端与 React 应用。',
    },
  },
  docs: {
    title: {en: 'Documentation - AppStruct', zh: '文档 - AppStruct'},
    description: {en: 'AppStruct guides, module contracts, operations notes, and technical records.', zh: 'AppStruct 使用指南、模块契约、生产运维说明与技术记录。'},
  },
  notFound: {
    title: {en: 'Page not found - AppStruct', zh: '页面不存在 - AppStruct'},
    description: {en: 'The requested page could not be found.', zh: '找不到请求的页面。'},
  },
};

export function langFromStorage(value: string | null): Lang {
  return value === 'zh' ? 'zh' : 'en';
}
