import { defineCollection } from 'astro:content';
import { glob } from 'astro/loaders';
import { z } from 'astro/zod';

const docs = defineCollection({
  loader: glob({ pattern: '*.md', base: './src/content/docs' }),
});

/** Problem-first pages; their titles and order are in lib/guides.ts. */
const guides = defineCollection({
  loader: glob({ pattern: '*.md', base: './src/content/guides' }),
});

/** The repository's own docs, so the Project file reference has one source. */
const reference = defineCollection({
  loader: glob({ pattern: 'configuration.md', base: '../docs' }),
});

const changelog = defineCollection({
  loader: glob({ pattern: '*.md', base: './src/content/changelog' }),
  schema: z.object({
    version: z.string(),
    title: z.string(),
    date: z.coerce.date(),
    released: z.boolean(),
  }),
});

export const collections = { docs, guides, reference, changelog };
