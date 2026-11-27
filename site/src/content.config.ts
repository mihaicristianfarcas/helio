// The documentation is the repository's own Markdown, read where it lies
// (lib/docs.mjs says which files), so the site and the repository never
// say different things.
import { defineCollection } from 'astro:content';
import { glob } from 'astro/loaders';
import { FILES, idOf } from './lib/docs.mjs';

export const collections = {
  docs: defineCollection({
    loader: glob({ base: '..', pattern: FILES, generateId: ({ entry }) => idOf(entry) }),
  }),
};
