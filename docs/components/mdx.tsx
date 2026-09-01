import defaultMdxComponents from 'fumadocs-ui/mdx';
import type { MDXComponents } from 'mdx/types';
import { BamlRunner } from './baml-runner';

export function getMDXComponents(components?: MDXComponents) {
  return {
    ...defaultMdxComponents,
    BamlRunner,
    ...components,
  } satisfies MDXComponents;
}

export const useMDXComponents = getMDXComponents;

declare global {
  type MDXProvidedComponents = ReturnType<typeof getMDXComponents>;
}
