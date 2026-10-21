import defaultMdxComponents from 'fumadocs-ui/mdx';
import type { MDXComponents } from 'mdx/types';

import { BamlProject, BamlSnippet } from '@/components/baml-snippet';
import { BridgeCompatibility } from '@/components/bridge-compatibility';
import { CodeBlock } from '@/components/code-block';
import { CodeComparison } from '@/components/code-comparison';
import { CodeExample } from '@/components/code-example';
import { DocsCard } from '@/components/docs-card';
import { LanguageTabs, ProviderTabs } from '@/components/language-tabs';
import {
  PerspectiveNote,
  PerspectiveSection,
} from '@/components/perspective-note';

export function useMDXComponents(components: MDXComponents): MDXComponents {
  return {
    ...defaultMdxComponents,
    BamlProject,
    BamlSnippet,
    BridgeCompatibility,
    CodeComparison,
    CodeExample,
    DocsCard,
    LanguageTabs,
    PerspectiveNote,
    PerspectiveSection,
    ProviderTabs,
    pre: CodeBlock,
    table: (props) => (
      <div className="docs-table-scroll">
        <table {...props} />
      </div>
    ),
    ...components,
  };
}
