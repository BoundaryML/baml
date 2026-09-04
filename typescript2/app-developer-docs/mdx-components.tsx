import defaultMdxComponents from 'fumadocs-ui/mdx';
import type { MDXComponents } from 'mdx/types';

import { BamlProject, BamlSnippet } from '@/components/baml-snippet';

export function useMDXComponents(components: MDXComponents): MDXComponents {
  return {
    ...defaultMdxComponents,
    BamlProject,
    BamlSnippet,
    ...components,
  };
}
