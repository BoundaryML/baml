import defaultMdxComponents from 'fumadocs-ui/mdx';
import type { MDXComponents } from 'mdx/types';

import { BamlProject, BamlSnippet } from '@/components/baml-snippet';
import { BridgeCompatibility } from '@/components/bridge-compatibility';
import { DocsCard } from '@/components/docs-card';

export function useMDXComponents(components: MDXComponents): MDXComponents {
  return {
    ...defaultMdxComponents,
    BamlProject,
    BamlSnippet,
    BridgeCompatibility,
    DocsCard,
    ...components,
  };
}
