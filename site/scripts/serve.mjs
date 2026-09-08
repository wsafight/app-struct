import {createReadStream} from 'node:fs';
import {stat} from 'node:fs/promises';
import {createServer} from 'node:http';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {deployment} from './deployment.mjs';

const dist = fileURLToPath(new URL('../dist', import.meta.url));
const {base} = deployment();
const port = Number(process.env.PORT || process.argv[2] || 4321);
const types = {
  '.css': 'text/css; charset=utf-8', '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8', '.json': 'application/json; charset=utf-8',
  '.md': 'text/markdown; charset=utf-8', '.svg': 'image/svg+xml',
  '.txt': 'text/plain; charset=utf-8', '.xml': 'application/xml; charset=utf-8',
  '.woff2': 'font/woff2',
};

function stripBase(pathname) {
  if (base === '/') return pathname;
  const prefix = base.replace(/\/$/, '');
  if (pathname === prefix || pathname === `${prefix}/`) return '/';
  return pathname.startsWith(`${prefix}/`) ? pathname.slice(prefix.length) : null;
}

createServer(async (req, res) => {
  const url = new URL(req.url || '/', `http://127.0.0.1:${port}`);
  const pathname = stripBase(decodeURIComponent(url.pathname));
  if (pathname === null) return void res.writeHead(404).end('Not found');
  const relative = pathname.replace(/^\//, '');
  const candidates = [path.join(dist, relative), path.join(dist, relative, 'index.html'), path.join(dist, `${relative.replace(/\/$/, '')}.html`)];
  for (const file of candidates) {
    try {
      if (!(await stat(file)).isFile()) continue;
      res.writeHead(200, {'Content-Type': types[path.extname(file)] || 'application/octet-stream'});
      createReadStream(file).pipe(res);
      return;
    } catch {}
  }
  res.writeHead(404, {'Content-Type': 'text/html; charset=utf-8'});
  createReadStream(path.join(dist, '404.html')).pipe(res);
}).listen(port, '127.0.0.1', () => console.log(`Serving ${dist} at http://127.0.0.1:${port}${base}`));
