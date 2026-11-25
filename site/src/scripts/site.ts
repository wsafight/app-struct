import {href, pages} from '../catalog';
import {ui, type Lang} from '../i18n';

const header = document.querySelector<HTMLElement>('#site-header');
const menu = document.querySelector<HTMLElement>('#mobile-menu');
const menuToggle = document.querySelector<HTMLButtonElement>('#menu-toggle');
const dialog = document.querySelector<HTMLDialogElement>('#search-dialog');
const searchInput = document.querySelector<HTMLInputElement>('[data-search-input]');
const results = document.querySelector<HTMLElement>('[data-search-results]');
const empty = document.querySelector<HTMLElement>('[data-search-empty]');

type SearchItem = {title: string; description: string; url: string};
type Pagefind = {
  init(): Promise<void>;
  search(query: string): Promise<{results: Array<{data(): Promise<{url: string; meta: {title?: string; lang?: string}; excerpt?: string}>}>}>;
};

const pageLang: Lang = document.documentElement.dataset.lang === 'zh' ? 'zh' : 'en';
const text = ui[pageLang];
const catalog: SearchItem[] = pages.map(page => ({
  title: page.title[pageLang],
  description: page.description[pageLang],
  url: href(page.slug, pageLang),
}));

let pagefind: Pagefind | undefined;
let lastScroll = window.scrollY;
let searchRevision = 0;

function storageSet(key: string, value: string) {
  try { localStorage.setItem(key, value); } catch {}
}

function setMenu(open: boolean) {
  if (!menu || !menuToggle) return;
  menu.classList.toggle('is-open', open);
  menu.setAttribute('aria-hidden', String(!open));
  menuToggle.setAttribute('aria-expanded', String(open));
  menuToggle.setAttribute('aria-label', open ? text.menuClose : text.menuOpen);
  header?.classList.toggle('menu-open', open);
  document.body.classList.toggle('menu-open', open);
}

function setTheme(theme: 'light' | 'dark') {
  document.documentElement.dataset.theme = theme;
  storageSet('appstruct-theme', theme);
  const themeMeta = document.querySelector('meta[name="theme-color"]');
  if (themeMeta) themeMeta.setAttribute('content', theme === 'light' ? '#f8faf9' : '#080a0c');
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
  } catch {
    pagefind = undefined;
  }
}

function renderItems(items: SearchItem[]) {
  if (!results || !empty) return;
  results.replaceChildren();
  empty.hidden = items.length > 0 || !searchInput?.value.trim();
  for (const item of items.slice(0, 8)) {
    const link = document.createElement('a');
    const title = document.createElement('strong');
    const excerpt = document.createElement('small');
    link.href = item.url;
    title.textContent = item.title;
    excerpt.textContent = item.description;
    link.append(title, excerpt);
    results.append(link);
  }
}

async function search(query: string) {
  const revision = ++searchRevision;
  const value = query.trim();
  if (!value) {
    renderItems([]);
    if (empty) empty.hidden = true;
    return;
  }
  searchInput?.setAttribute('aria-busy', 'true');
  try {
    if (pagefind) {
      const found = await pagefind.search(value);
      const allItems = await Promise.all(
        found.results.slice(0, 16).map(async result => {
          const data = await result.data();
          const itemLang: Lang = data.meta.lang === 'zh' || data.url.includes('/zh/') ? 'zh' : 'en';
          const excerpt = data.excerpt?.replace(/<[^>]+>/g, '') || '';
          return {
            lang: itemLang,
            title: data.meta.title || data.url,
            description: excerpt,
            url: data.url,
          };
        }),
      );
      if (revision !== searchRevision) return;
      const sameLang = allItems.filter(item => item.lang === pageLang);
      const chosen = sameLang.length ? sameLang : allItems;
      if (chosen.length) {
        renderItems(chosen.slice(0, 8));
        return;
      }
    }
    const lower = value.toLowerCase();
    const items = catalog.filter(item => `${item.title} ${item.description}`.toLowerCase().includes(lower));
    if (revision === searchRevision) renderItems(items);
  } finally {
    if (revision === searchRevision) searchInput?.removeAttribute('aria-busy');
  }
}

window.addEventListener('scroll', () => {
  if (!header) return;
  const current = window.scrollY;
  const reading = document.body.classList.contains('docs-body');
  header.classList.toggle('is-hidden', !reading && current > lastScroll && current > 110 && !document.body.classList.contains('menu-open'));
  lastScroll = current;
}, {passive: true});

menuToggle?.addEventListener('click', () => setMenu(!menu?.classList.contains('is-open')));
menu?.querySelectorAll('a').forEach(link => link.addEventListener('click', () => setMenu(false)));
document.querySelectorAll<HTMLButtonElement>('[data-theme-toggle]').forEach(button => {
  button.addEventListener('click', () => setTheme(document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark'));
});

document.querySelectorAll<HTMLButtonElement>('[data-search-open]').forEach(button => {
  button.addEventListener('click', async () => {
    await loadPagefind();
    dialog?.showModal();
    searchInput?.focus();
  });
});

let selectedSearchIndex = -1;
function updateSearchSelection(nextIndex: number) {
  const links = [...(results?.querySelectorAll<HTMLAnchorElement>('a') ?? [])];
  if (!links.length) { selectedSearchIndex = -1; return; }
  selectedSearchIndex = (nextIndex + links.length) % links.length;
  links.forEach((link, index) => {
    const active = index === selectedSearchIndex;
    link.classList.toggle('is-selected', active);
    if (active) link.scrollIntoView({block: 'nearest'});
  });
}

document.addEventListener('keydown', event => {
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
    event.preventDefault();
    if (dialog?.open) dialog.close();
    else document.querySelector<HTMLButtonElement>('[data-search-open]')?.click();
    return;
  }
  if (event.key === '/' && !(event.target instanceof HTMLInputElement) && !(event.target instanceof HTMLTextAreaElement)) {
    event.preventDefault();
    document.querySelector<HTMLButtonElement>('[data-search-open]')?.click();
  }
  if (event.key === 'Escape') setMenu(false);
});

searchInput?.addEventListener('input', () => {
  selectedSearchIndex = -1;
  void search(searchInput.value);
});
searchInput?.addEventListener('keydown', event => {
  if (event.key === 'ArrowDown') { event.preventDefault(); updateSearchSelection(selectedSearchIndex + 1); }
  else if (event.key === 'ArrowUp') { event.preventDefault(); updateSearchSelection(selectedSearchIndex - 1); }
  else if (event.key === 'Enter' && selectedSearchIndex >= 0) {
    const active = results?.querySelectorAll<HTMLAnchorElement>('a')[selectedSearchIndex];
    if (active) { event.preventDefault(); active.click(); }
  }
});
dialog?.addEventListener('click', event => { if (event.target === dialog) dialog.close(); });

// Hero CLI Strip
document.querySelectorAll<HTMLElement>('[data-cli-strip]').forEach(strip => {
  const tabs = [...strip.querySelectorAll<HTMLButtonElement>('[data-cli-tab]')];
  const cmds = [...strip.querySelectorAll<HTMLElement>('[data-cli-cmd]')];
  const copyBtn = strip.querySelector<HTMLButtonElement>('[data-copy-cli]');
  tabs.forEach(tab => tab.addEventListener('click', () => {
    const target = tab.dataset.cliTab;
    tabs.forEach(item => {
      const current = item.dataset.cliTab === target;
      item.classList.toggle('is-current', current);
      item.setAttribute('aria-selected', String(current));
    });
    cmds.forEach(cmd => { cmd.hidden = cmd.dataset.cliCmd !== target; });
  }));
  copyBtn?.addEventListener('click', async () => {
    const active = cmds.find(cmd => !cmd.hidden);
    const value = active?.textContent?.trim() || '';
    if (!value) return;
    try {
      await navigator.clipboard.writeText(value);
      copyBtn.textContent = text.copied;
      window.setTimeout(() => { copyBtn.textContent = text.copy; }, 1600);
    } catch {}
  });
});

// Workbench Tabs + Copy
document.querySelectorAll<HTMLElement>('[data-workbench]').forEach(workbench => {
  const tabs = [...workbench.querySelectorAll<HTMLButtonElement>('[data-workbench-tab]')];
  const panels = [...workbench.querySelectorAll<HTMLElement>('[data-workbench-panel]')];
  const copyBtn = workbench.querySelector<HTMLButtonElement>('[data-workbench-copy]');
  tabs.forEach(tab => tab.addEventListener('click', () => {
    const target = tab.dataset.workbenchTab;
    tabs.forEach(item => {
      const current = item.dataset.workbenchTab === target;
      item.classList.toggle('is-current', current);
      item.setAttribute('aria-selected', String(current));
    });
    panels.forEach(panel => { panel.hidden = panel.dataset.workbenchPanel !== target; });
  }));
  copyBtn?.addEventListener('click', async () => {
    const active = panels.find(panel => !panel.hidden);
    const code = active?.querySelector('code')?.textContent || active?.textContent || '';
    if (!code) return;
    try {
      await navigator.clipboard.writeText(code);
      copyBtn.textContent = text.copied;
      window.setTimeout(() => { copyBtn.textContent = text.copy; }, 1600);
    } catch {}
  });
});

// Copy code buttons in docs
document.querySelectorAll<HTMLElement>('.prose pre').forEach(block => {
  if (block.querySelector('.copy-code')) return;
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'copy-code';
  button.textContent = text.copy;
  button.addEventListener('click', async () => {
    const code = block.querySelector('code')?.textContent || block.textContent || '';
    try {
      await navigator.clipboard.writeText(code);
      button.textContent = text.copied;
      window.setTimeout(() => { button.textContent = text.copy; }, 1600);
    } catch {}
  });
  block.append(button);
});

// Heading anchor links in docs
document.querySelectorAll<HTMLElement>('.prose :is(h2,h3,h4)[id]').forEach(heading => {
  if (heading.querySelector('.heading-anchor')) return;
  const anchor = document.createElement('a');
  anchor.className = 'heading-anchor';
  anchor.href = `#${heading.id}`;
  anchor.setAttribute('aria-label', text.linkHeading);
  heading.append(anchor);
});

// Responsive doc panels
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

// Scroll active sidebar link into view
const currentDoc = document.querySelector<HTMLAnchorElement>('.docs-sidebar a[aria-current="page"]');
const sidebar = currentDoc?.closest('.docs-sidebar');
if (currentDoc && sidebar instanceof HTMLElement) {
  const linkBox = currentDoc.getBoundingClientRect();
  const barBox = sidebar.getBoundingClientRect();
  if (linkBox.top < barBox.top || linkBox.bottom > barBox.bottom) {
    currentDoc.scrollIntoView({block: 'center'});
  }
}

// Active TOC observer
const tocLinks = [...document.querySelectorAll<HTMLAnchorElement>('.docs-toc a')];
const headings = tocLinks
  .map(link => {
    const id = decodeURIComponent(link.getAttribute('href')?.slice(1) || '');
    return id ? document.getElementById(id) : null;
  })
  .filter((node): node is HTMLElement => Boolean(node));

if (tocLinks.length && headings.length) {
  const setActive = (id: string) => {
    tocLinks.forEach(link => {
      link.classList.toggle('is-active', decodeURIComponent(link.getAttribute('href')?.slice(1) || '') === id);
    });
  };
  const observer = new IntersectionObserver(
    entries => {
      const visible = entries.find(entry => entry.isIntersecting);
      if (visible) setActive(visible.target.id);
    },
    {rootMargin: '-18% 0px -70% 0px'},
  );
  headings.forEach(heading => observer.observe(heading));
  setActive(headings[0].id);
}
