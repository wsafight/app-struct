import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {visit} from 'unist-util-visit';
import {pages} from './catalog.mjs';
import {deployment} from './deployment.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url)).replace(/[/\\]$/, '');
const pageBySource = new Map();
for (const page of pages) {
  pageBySource.set(page.source, page);
  if (page.translation) pageBySource.set(page.translation, page);
}

function siteHref(slug) {
  const base = deployment().base.replace(/\/$/, '');
  return `${base}/${slug}/`;
}

export function resolveLink(url, file) {
  if (/^(?:[a-z][a-z\d+.-]*:|\/\/|#)/i.test(url)) return url;
  const [relative, hash = ''] = url.split('#');
  const suffix = hash ? `#${hash}` : '';
  const target = path.relative(root, path.resolve(path.dirname(file), decodeURI(relative))).split(path.sep).join('/');
  const page = pageBySource.get(target);
  if (page) {
    const from = path.relative(root, file).split(path.sep).join('/');
    const sourcePage = pageBySource.get(from);
    const query = sourcePage && from === sourcePage.translation ? '?lang=zh' : '';
    return `${siteHref(page.slug)}${query}${suffix}`;
  }
  if (target.startsWith('..') || path.isAbsolute(target)) throw new Error(`Link escapes repository: ${url}`);
  const base = deployment().base.replace(/\/$/, '');
  return `${base}/source/${target.split('/').map(encodeURIComponent).join('/')}${suffix}`;
}

export function docsMarkdown() {
  return (tree, file) => {
    const firstHeading = tree.children.findIndex(node => node.type === 'heading');
    if (firstHeading >= 0 && tree.children[firstHeading].depth === 1) tree.children.splice(firstHeading, 1);
    visit(tree, node => {
      if (['link', 'image', 'definition'].includes(node.type)) node.url = resolveLink(node.url, file.path);
    });
  };
}
