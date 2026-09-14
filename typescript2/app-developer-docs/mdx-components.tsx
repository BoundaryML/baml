import defaultMdxComponents from 'fumadocs-ui/mdx';
import type { MDXComponents } from 'mdx/types';

import { BamlProject, BamlSnippet } from '@/components/baml-snippet';
import { BridgeCompatibility } from '@/components/bridge-compatibility';
import { CodeBlock } from '@/components/code-block';
import { CodeExample } from '@/components/code-example';
import { DocsCard } from '@/components/docs-card';
import { LanguageTabs } from '@/components/language-tabs';

export function useMDXComponents(components: MDXComponents): MDXComponents {
  return {
    ...defaultMdxComponents,
    BamlProject,
    BamlSnippet,
    BridgeCompatibility,
    CodeExample,
    DocsCard,
    LanguageTabs,
    pre: CodeBlock,
    table: (props) => (
      <div className="docs-table-scroll">
        <table {...props} />
      </div>
    ),
    ...components,
  };
}
