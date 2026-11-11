import type {APIRoute} from 'astro';

export const GET: APIRoute = ({site}) => {
  const base = import.meta.env.BASE_URL;
  const sitemap = site ? new URL(`${base}sitemap-index.xml`, site).href : `${base}sitemap-index.xml`;
  return new Response(`User-agent: *\nAllow: /\nSitemap: ${sitemap}\n`, {headers: {'Content-Type': 'text/plain; charset=utf-8'}});
};
