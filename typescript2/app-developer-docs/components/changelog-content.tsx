import { Children, createElement, isValidElement, type ReactNode } from 'react';
import ReactMarkdown from 'react-markdown';
import { changelogHeadingId } from '@/lib/changelog/loader';

function renderedText(node: ReactNode): string {
  return Children.toArray(node)
    .map((child) =>
      isValidElement<{ children?: ReactNode }>(child)
        ? renderedText(child.props.children)
        : String(child),
    )
    .join('');
}

export function ChangelogContent({ markdown }: { markdown: string }) {
  return createElement(
    ReactMarkdown,
    {
      components: {
        h2: ({ children }) => {
          const id = changelogHeadingId(renderedText(children));
          return createElement('h2', { id }, children);
        },
      },
    },
    markdown,
  );
}
