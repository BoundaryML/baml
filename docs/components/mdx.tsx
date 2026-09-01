import defaultMdxComponents from 'fumadocs-ui/mdx';
import type { MDXComponents } from 'mdx/types';
import { BamlRunner } from './baml-runner';
import { BookListing } from './book-listing';
import { BookQuiz } from './book-quiz';

export function getMDXComponents(components?: MDXComponents) {
  return {
    ...defaultMdxComponents,
    BamlRunner,
    BookListing,
    BookQuiz,
    ...components,
  } satisfies MDXComponents;
}

export const useMDXComponents = getMDXComponents;

declare global {
  type MDXProvidedComponents = ReturnType<typeof getMDXComponents>;
}
