import {href, pages} from '../catalog';
import {langFromStorage, ui, type Lang} from '../i18n';

const header = document.querySelector<HTMLElement>('#site-header');
const menu = document.querySelector<HTMLElement>('#mobile-menu');
const menuToggle = document.querySelector<HTMLButtonElement>('#menu-toggle');
const dialog = document.querySelector<HTMLDialogElement>('#search-dialog');
const searchInput = document.querySelector<HTMLInputElement>('[data-search-input]');
const results = document.querySelector<HTMLElement>('[data-search-results]');
const empty = document.querySelector<HTMLElement>('[data-search-empty]');

type SearchItem = {title: {en: string; zh: string}; description: {en: string; zh: string}; url: string};
type Pagefind = {init(): Promise<void>; search(query: string): Promise<{results: Array<{data(): Promise<{url: string; meta: {title?: string}; excerpt?: string}>}>}>};
const catalog: SearchItem[] = pages.map(page => ({title: page.title, description: page.description, url: href(page.slug)}));
let pagefind: Pagefind | undefined;
let lastScroll = window.scrollY;
let searchRevision = 0;

function currentLang(): Lang {
  return langFromStorage(document.documentElement.dataset.lang || localStorage.getItem('appstruct-lang'));
}

function copy(lang = currentLang()) { return ui[lang]; }

function applyLang(lang: Lang, persist = true) {
  const root = document.documentElement;
  root.dataset.lang = lang;
  root.lang = lang === 'zh' ? 'zh-CN' : 'en';
  if (persist) {
    localStorage.setItem('appstruct-lang', lang);
    const url = new URL(location.href);
    if (lang === 'zh') url.searchParams.set('lang', 'zh');
    else url.searchParams.delete('lang');
    history.replaceState(null, '', `${url.pathname}${url.search}${url.hash}`);
  }
  const text = ui[lang];
  const title = root.getAttribute(lang === 'zh' ? 'data-title-zh' : 'data-title-en');
  const description = root.getAttribute(lang === 'zh' ? 'data-desc-zh' : 'data-desc-en');
  if (title) document.title = title;
  const meta = document.querySelector('meta[name="description"]');
  if (meta && description) meta.setAttribute('content', description);
  document.querySelectorAll<HTMLElement>('[data-i18n-aria]').forEach(node => {
    const key = node.dataset.i18nAria as keyof typeof text | undefined;
    if (key && text[key]) node.setAttribute('aria-label', text[key]);
  });
  document.querySelectorAll<HTMLInputElement>('[data-i18n-placeholder]').forEach(node => {
    const key = node.dataset.i18nPlaceholder as keyof typeof text | undefined;
    if (key && text[key]) node.setAttribute('placeholder', text[key]);
  });
  if (menuToggle && !menu?.classList.contains('is-open')) menuToggle.setAttribute('aria-label', text.menuOpen);
  document.querySelectorAll<HTMLButtonElement>('.copy-code').forEach(button => {
    if (button.dataset.copied !== 'true') button.textContent = text.copy;
  });
  document.querySelectorAll<HTMLAnchorElement>('.heading-anchor').forEach(anchor => {
    anchor.setAttribute('aria-label', text.linkHeading);
  });
  if (dialog?.open && searchInput?.value) void search(searchInput.value);
  watchToc();
}

function setMenu(open: boolean) {
  if (!menu || !menuToggle) return;
  menu.classList.toggle('is-open', open);
  menu.setAttribute('aria-hidden', String(!open));
  menuToggle.setAttribute('aria-expanded', String(open));
  menuToggle.setAttribute('aria-label', open ? copy().menuClose : copy().menuOpen);
  header?.classList.toggle('menu-open', open);
  document.body.classList.toggle('menu-open', open);
}

function setTheme(theme: 'light' | 'dark') {
  document.documentElement.dataset.theme = theme;
  localStorage.setItem('appstruct-theme', theme);
}

function moveIndicator(target?: HTMLElement | null) {
  const nav = document.querySelector<HTMLElement>('.desktop-nav');
  const indicator = document.querySelector<HTMLElement>('.nav-indicator');
  if (!nav || !indicator || !target) { indicator?.style.setProperty('opacity', '0'); return; }
  const navBox = nav.getBoundingClientRect();
  const box = target.getBoundingClientRect();
  indicator.style.width = `${box.width}px`;
  indicator.style.transform = `translateX(${box.left - navBox.left}px)`;
  indicator.style.opacity = '1';
}

async function loadPagefind() {
  if (pagefind || !dialog?.dataset.pagefindUrl) return;
  try {
    const loaded = await Promise.race([
      import(/* @vite-ignore */ dialog.dataset.pagefindUrl),
      new Promise<never>((_, reject) => window.setTimeout(() => reject(new Error('Search index timeout')), 1500)),
    ]);
    pagefind = loaded as Pagefind;
    await pagefind.init();
  } catch { pagefind = undefined; }
}

function renderItems(items: SearchItem[]) {
  if (!results || !empty) return;
  results.replaceChildren();
  empty.hidden = items.length > 0 || !searchInput?.value.trim();
  for (const item of items.slice(0, 8)) {
    const link = document.createElement('a');
    const title = document.createElement('strong');
    const excerpt = document.createElement('small');
    const lang = currentLang();
    link.href = item.url;
    title.textContent = item.title[lang];
    excerpt.textContent = item.description[lang];
    link.append(title, excerpt);
    results.append(link);
  }
}

async function search(query: string) {
  const revision = ++searchRevision;
  const value = query.trim();
  if (!value) { renderItems([]); if (empty) empty.hidden = true; return; }
  searchInput?.setAttribute('aria-busy', 'true');
  try {
    if (pagefind) {
      const found = await pagefind.search(value);
      const items = await Promise.all(found.results.slice(0, 8).map(async result => {
        const data = await result.data();
        const pathname = new URL(data.url, location.href).pathname.replace(/\/$/, '');
        const localized = catalog.find(item => new URL(item.url, location.href).pathname.replace(/\/$/, '') === pathname);
        const excerpt = data.excerpt?.replace(/<[^>]+>/g, '') || '';
        return {
          title: localized?.title ?? {en: data.meta.title || data.url, zh: data.meta.title || data.url},
          description: {en: excerpt || localized?.description.en || '', zh: excerpt || localized?.description.zh || ''},
          url: data.url,
        };
      }));
      if (revision !== searchRevision) return;
      if (items.length) { renderItems(items); return; }
    }
    const lower = value.toLowerCase();
    const items = catalog.filter(item => `${item.title.en} ${item.title.zh} ${item.description.en} ${item.description.zh}`.toLowerCase().includes(lower));
    if (revision === searchRevision) renderItems(items);
  } finally {
    if (revision === searchRevision) searchInput?.removeAttribute('aria-busy');
  }
}

header?.querySelectorAll('.desktop-nav a').forEach(link => link.addEventListener('mouseenter', () => moveIndicator(link as HTMLElement)));
header?.querySelector('.desktop-nav')?.addEventListener('mouseleave', () => moveIndicator(header.querySelector('.desktop-nav a[aria-current="page"]') as HTMLElement | null));
moveIndicator(header?.querySelector('.desktop-nav a[aria-current="page"]') as HTMLElement | null);

window.addEventListener('scroll', () => {
  if (!header) return;
  const current = window.scrollY;
  const reading = document.body.classList.contains('docs-body');
  header.classList.toggle('is-scrolled', current > 16);
  header.classList.toggle('is-hidden', !reading && current > lastScroll && current > 90 && !document.body.classList.contains('menu-open'));
  lastScroll = current;
}, {passive: true});

menuToggle?.addEventListener('click', () => setMenu(!menu?.classList.contains('is-open')));
menu?.querySelectorAll('a').forEach(link => link.addEventListener('click', () => setMenu(false)));
document.querySelectorAll<HTMLButtonElement>('[data-theme-toggle]').forEach(button => button.addEventListener('click', () => setTheme(document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark')));
document.querySelectorAll<HTMLButtonElement>('[data-lang-toggle]').forEach(button => button.addEventListener('click', () => applyLang(currentLang() === 'zh' ? 'en' : 'zh')));
applyLang(currentLang(), false);

document.querySelectorAll<HTMLButtonElement>('[data-search-open]').forEach(button => button.addEventListener('click', async () => {
  await loadPagefind(); dialog?.showModal(); searchInput?.focus();
}));
document.addEventListener('keydown', event => {
  if (event.key === '/' && !(event.target instanceof HTMLInputElement) && !(event.target instanceof HTMLTextAreaElement)) {
    event.preventDefault(); document.querySelector<HTMLButtonElement>('[data-search-open]')?.click();
  }
  if (event.key === 'Escape') setMenu(false);
});
searchInput?.addEventListener('input', () => void search(searchInput.value));
dialog?.addEventListener('click', event => { if (event.target === dialog) dialog.close(); });

document.querySelectorAll<HTMLElement>('.capability-panel').forEach(panel => {
  const activate = () => {
    document.querySelectorAll('.capability-panel.is-active').forEach(item => item.classList.remove('is-active'));
    panel.classList.add('is-active');
  };
  panel.addEventListener('mouseenter', activate);
  panel.addEventListener('focusin', activate);
  panel.querySelector('button')?.addEventListener('click', activate);
});

document.querySelectorAll<HTMLElement>('.prose pre').forEach(block => {
  if (block.querySelector('.copy-code')) return;
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'copy-code';
  button.textContent = copy().copy;
  button.addEventListener('click', async () => {
    const code = block.querySelector('code')?.textContent || block.textContent || '';
    await navigator.clipboard.writeText(code);
    button.dataset.copied = 'true';
    button.textContent = copy().copied;
    window.setTimeout(() => { delete button.dataset.copied; button.textContent = copy().copy; }, 1600);
  });
  block.append(button);
});

document.querySelectorAll('.prose.i18n-zh :is(h2,h3,h4)[id]').forEach(heading => {
  if (!heading.id.startsWith('zh-')) heading.id = `zh-${heading.id}`;
});
document.querySelectorAll('.docs-toc.i18n-zh a[href^="#"]').forEach(link => {
  const hash = decodeURIComponent(link.getAttribute('href')?.slice(1) || '');
  if (hash && !hash.startsWith('zh-')) link.setAttribute('href', `#zh-${hash}`);
});

document.querySelectorAll<HTMLElement>('.prose :is(h2,h3,h4)[id]').forEach(heading => {
  if (heading.querySelector('.heading-anchor')) return;
  const anchor = document.createElement('a');
  anchor.className = 'heading-anchor';
  anchor.href = `#${heading.id}`;
  anchor.dataset.i18nAria = 'linkHeading';
  anchor.setAttribute('aria-label', copy().linkHeading);
  heading.append(anchor);
});

function syncDocPanels() {
  const compactNav = window.matchMedia('(max-width: 780px)').matches;
  const compactToc = window.matchMedia('(max-width: 1080px)').matches;
  document.querySelectorAll<HTMLDetailsElement>('.docs-nav-panel').forEach(panel => {
    if (!compactNav) panel.open = true;
  });
  document.querySelectorAll<HTMLDetailsElement>('.docs-outline').forEach(panel => {
    if (!compactToc) panel.open = true;
  });
}
syncDocPanels();
window.matchMedia('(max-width: 780px)').addEventListener('change', syncDocPanels);
window.matchMedia('(max-width: 1080px)').addEventListener('change', syncDocPanels);

const currentDoc = document.querySelector<HTMLAnchorElement>('.docs-sidebar a[aria-current="page"]');
const sidebar = currentDoc?.closest('.docs-sidebar');
if (currentDoc && sidebar instanceof HTMLElement) {
  const linkBox = currentDoc.getBoundingClientRect();
  const sideBox = sidebar.getBoundingClientRect();
  sidebar.scrollTop += linkBox.top - sideBox.top - sideBox.height / 3;
}

document.querySelectorAll('.docs-outline a').forEach(link => {
  link.addEventListener('click', () => {
    const details = link.closest('details');
    if (details && window.matchMedia('(max-width: 1080px)').matches) details.open = false;
  });
});

let tocObserver: IntersectionObserver | undefined;
function watchToc() {
  tocObserver?.disconnect();
  const tocLinks = [...document.querySelectorAll<HTMLAnchorElement>(`.docs-toc.i18n-${currentLang()} a`)];
  const headings = tocLinks.map(link => document.querySelector(decodeURIComponent(link.hash))).filter((node): node is HTMLElement => node instanceof HTMLElement);
  if (!tocLinks.length || !headings.length) return;
  tocObserver = new IntersectionObserver(entries => {
    const visible = entries.filter(entry => entry.isIntersecting).at(-1);
    if (!visible?.target.id) return;
    tocLinks.forEach(link => link.classList.toggle('is-active', decodeURIComponent(link.hash) === `#${visible.target.id}`));
  }, {rootMargin: '-20% 0px -70% 0px', threshold: 0.1});
  headings.forEach(heading => tocObserver?.observe(heading));
}
watchToc();
