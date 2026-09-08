import {copyFile, mkdir, readFile, rm, writeFile} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {remark} from 'remark';
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

for (const page of pages) {
  await renderDoc('en', page, page.source);
  await renderDoc('zh', page, page.translation || page.source);
  for (const sourceName of new Set([page.source, page.translation].filter(Boolean))) {
    const destination = path.join(publicSource, sourceName);
    await mkdir(path.dirname(destination), {recursive: true});
    await copyFile(path.join(root, sourceName), destination);
  }
}

const cargo = await readFile(path.join(root, 'Cargo.toml'), 'utf8');
const version = cargo.match(/\[workspace\.package\][\s\S]*?\nversion\s*=\s*"([^"]+)"/)?.[1] || 'unknown';
await writeFile(path.join(root, 'site/public/version.json'), `${JSON.stringify({version})}\n`);
