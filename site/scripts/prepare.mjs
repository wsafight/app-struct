import {copyFile, mkdir, readFile, rm, writeFile} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {remark} from 'remark';
import {visit} from 'unist-util-visit';
import {pages} from './catalog.mjs';
import {docsMarkdown} from './markdown.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url)).replace(/[/\\]$/, '');
const generated = path.join(root, 'site/.generated/docs');
const publicSource = path.join(root, 'site/public/source');
const processor = remark().use(docsMarkdown);

await rm(generated, {recursive: true, force: true});
await rm(publicSource, {recursive: true, force: true});

async function renderDoc(lang, page, sourceName) {
  const sourcePath = path.join(root, sourceName);
  const value = await readFile(sourcePath, 'utf8');
  const rendered = await processor.process({path: sourcePath, value});
  const destination = path.join(generated, lang, `${page.slug}.md`);
  await mkdir(path.dirname(destination), {recursive: true});
  await writeFile(destination, String(rendered));
}

await Promise.all(
  pages.flatMap((page) => [
    renderDoc('en', page, page.source),
    renderDoc('zh', page, page.translation || page.source),
  ]),
);

// Markdown links to files outside the rendered catalog are published under /source/.
// Copy their complete local-link closure so those links cannot point at missing files.
const copiedSources = new Set();
const pending = [];
for (const page of pages) {
  for (const name of new Set([page.source, page.translation].filter(Boolean))) {
    pending.push(name);
  }
}
while (pending.length) {
  const batch = [];
  for (const name of pending.splice(0, pending.length)) {
    if (!copiedSources.has(name)) {
      copiedSources.add(name);
      batch.push(name);
    }
  }
  await Promise.all(
    batch.map(async (sourceName) => {
      const sourcePath = path.join(root, sourceName);
      const destination = path.join(publicSource, sourceName);
      const isMarkdown = sourceName.endsWith('.md');
      const [, value] = await Promise.all([
        mkdir(path.dirname(destination), {recursive: true}).then(() =>
          copyFile(sourcePath, destination),
        ),
        isMarkdown ? readFile(sourcePath, 'utf8') : Promise.resolve(null),
      ]);
      if (value === null) return;

      const tree = processor.parse({path: sourcePath, value});
      visit(tree, (node) => {
        if (!['link', 'image', 'definition'].includes(node.type)) return;
        const url = node.url || '';
        if (/^(?:[a-z][a-z\d+.-]*:|\/\/|#)/i.test(url)) return;
        const relative = url.split('#')[0];
        if (!relative) return;
        const targetPath = path.resolve(path.dirname(sourcePath), decodeURI(relative));
        const target = path.relative(root, targetPath).split(path.sep).join('/');
        if (target.startsWith('..') || path.isAbsolute(target)) {
          throw new Error(`Link escapes repository: ${url}`);
        }
        if (!copiedSources.has(target)) pending.push(target);
      });
    }),
  );
}

const cargo = await readFile(path.join(root, 'Cargo.toml'), 'utf8');
const version = cargo.match(/\[workspace\.package\][\s\S]*?\nversion\s*=\s*"([^"]+)"/)?.[1] || 'unknown';
await writeFile(path.join(root, 'site/public/version.json'), `${JSON.stringify({version})}\n`);
