import {groups, pages} from '../scripts/catalog.mjs';
import type {Lang} from './i18n';

export {groups, pages};
export type DocGroup = (typeof groups)[number];
export type DocPage = (typeof pages)[number];

export const href = (slug = '', lang: Lang = 'en') => {
  const base = import.meta.env.BASE_URL.replace(/\/$/, '');
  const prefix = lang === 'zh' ? '/zh' : '';
  const clean = slug.replace(/^\/+|\/+$/g, '');
  if (!clean) return `${base}${prefix}/`;
  return `${base}${prefix}/${clean}/`;
};

export const switchLangHref = (pathname: string, targetLang: Lang) => {
  const base = import.meta.env.BASE_URL.replace(/\/$/, '');
  let rel = pathname;
  if (base && rel.startsWith(base)) rel = rel.slice(base.length);
  if (!rel.startsWith('/')) rel = `/${rel}`;
  if (rel === '/404' || rel === '/404/' || rel === '/404.html') {
    return targetLang === 'zh' ? `${base}/zh/` : `${base}/`;
  }
  const isZh = rel === '/zh' || rel.startsWith('/zh/');
  if (targetLang === 'zh') {
    if (isZh) return `${base}${rel}`;
    return `${base}/zh${rel === '/' ? '/' : rel}`;
  }
  if (!isZh) return `${base}${rel}`;
  const stripped = rel.replace(/^\/zh(?=\/|$)/, '') || '/';
  return `${base}${stripped}`;
};

export const groupedPages = groups.map(group => ({
  ...group,
  pages: pages.filter(page => page.group === group.id),
}));

export function neighbors(slug: string) {
  const index = pages.findIndex(page => page.slug === slug);
  return {
    previous: index > 0 ? pages[index - 1] : undefined,
    next: index >= 0 && index < pages.length - 1 ? pages[index + 1] : undefined,
  };
}
