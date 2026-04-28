/**
 * Renders a $prompt_ast value — shows chat messages with roles and content.
 */

import type { FC } from 'react';
import type { BamlJsPromptAst, BamlJsPromptAstSimple, BamlJsPromptAstMessage } from '@b/pkg-proto';
import type { ResultRendererProps } from '../result-renderers';
import { MediaRenderer } from './Media';
import { CodeBlock } from '../components/ui/code-block';

function isPromptAst(value: unknown): value is BamlJsPromptAst {
  if (value == null || typeof value !== 'object') return false;
  const baml = (value as Record<string, unknown>).$baml;
  if (baml == null || typeof baml !== 'object') return false;
  return (baml as Record<string, unknown>).type === '$prompt_ast';
}

const roleCls: Record<string, string> = {
  system: 'text-vsc-yellow',
  user: 'text-vsc-green',
  assistant: 'text-vsc-accent',
};

const SimpleContent: FC<{ node: BamlJsPromptAstSimple }> = ({ node }) => {
  switch (node.content_type) {
    case 'string':
      return <span className="whitespace-pre-wrap">{node.value}</span>;
    case 'media':
      return <MediaRenderer value={node.value} />;
    case 'multiple':
      return (
        <div className="space-y-1">
          {node.value.map((item, i) => (
            <SimpleContent key={i} node={item} />
          ))}
        </div>
      );
    default:
      return <span className="text-vsc-text-faint">(unknown content)</span>;
  }
};

const MessageBlock: FC<{ msg: BamlJsPromptAstMessage }> = ({ msg }) => {
  const roleColor = roleCls[msg.role] ?? 'text-vsc-text-muted';
  return (
    <div className="rounded border border-vsc-border overflow-hidden">
      <div className="flex items-center gap-1.5 px-2 py-1 bg-vsc-surface border-b border-vsc-border-subtle">
        <span className={`font-semibold text-[11px] uppercase tracking-wide ${roleColor}`}>
          {msg.role}
        </span>
      </div>
      <div className="px-2 py-1.5 text-xs leading-relaxed text-vsc-text">
        {msg.content ? <SimpleContent node={msg.content} /> : <span className="text-vsc-text-faint">(empty)</span>}
      </div>
    </div>
  );
};

const AstNode: FC<{ node: BamlJsPromptAst }> = ({ node }) => {
  switch (node.content_type) {
    case 'simple':
      return <SimpleContent node={node.value} />;
    case 'message':
      return <MessageBlock msg={node.value} />;
    case 'multiple':
      return (
        <div className="space-y-1.5">
          {node.value.map((item, i) => (
            <AstNode key={i} node={item} />
          ))}
        </div>
      );
    default:
      return <span className="text-vsc-text-faint">(unknown prompt ast)</span>;
  }
};

export const PromptAstRenderer: FC<ResultRendererProps> = ({ value, displayMode }) => {
  if (!isPromptAst(value)) {
    return <CodeBlock>{JSON.stringify(value, null, 2)}</CodeBlock>;
  }

  if (displayMode === 'inline') {
    return <span className="font-vsc-mono text-xs text-vsc-text-faint">&lt;prompt_ast&gt;</span>;
  }

  return (
    <div className="space-y-1.5">
      <AstNode node={value} />
    </div>
  );
};
