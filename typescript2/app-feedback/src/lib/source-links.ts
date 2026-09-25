import type { SourceCitation } from './investigation';

interface Node {
  type: string;
  value?: string;
  url?: string;
  children?: Node[];
}

/** Link inspected filenames in prose and inline code, without nesting links or editing code blocks. */
export function sourceLinks(citations: SourceCitation[]) {
  return () => (tree: Node) => {
    if (!citations.length) return;
    const byName = new Map(citations.map((c) => [c.name, c.href]));
    const escaped = [...byName.keys()]
      .sort((a, b) => b.length - a.length)
      .map((name) => name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
    const pattern = new RegExp(
      `(?<![\\w./-])(?:${escaped.join('|')})(?![\\w/-]|\\.[A-Za-z0-9])`,
      'g',
    );
    function visit(node: Node) {
      if (
        ['link', 'linkReference', 'code', 'html'].includes(node.type) ||
        !node.children
      )
        return;
      node.children = node.children.flatMap((child) => {
        if (
          (child.type !== 'text' && child.type !== 'inlineCode') ||
          !child.value
        ) {
          visit(child);
          return [child];
        }
        const parts: Node[] = [];
        let offset = 0;
        for (const match of child.value.matchAll(pattern)) {
          if (match.index! > offset)
            parts.push({
              type: child.type,
              value: child.value.slice(offset, match.index),
            });
          parts.push({
            children: [{ type: child.type, value: match[0] }],
            type: 'link',
            url: byName.get(match[0]),
          });
          offset = match.index! + match[0].length;
        }
        if (offset < child.value.length)
          parts.push({ type: child.type, value: child.value.slice(offset) });
        return parts.length ? parts : [child];
      });
    }
    visit(tree);
  };
}
