import {defineCollection} from 'astro:content';
import {glob} from 'astro/loaders';

const generateId = ({entry}: {entry: string}) => entry.replace(/\.md$/, '');

const docsEn = defineCollection({
  loader: glob({pattern: '**/*.md', base: '.generated/docs/en', generateId}),
});

const docsZh = defineCollection({
  loader: glob({pattern: '**/*.md', base: '.generated/docs/zh', generateId}),
});

export const collections = {docsEn, docsZh};
