import { createElement } from 'react';
import ReactMarkdown from 'react-markdown';

export function ChangelogContent({
  headingIds,
  markdown,
}: {
  headingIds: Array<string | undefined>;
  markdown: string;
}) {
  let headingIndex = 0;
  return createElement(
    ReactMarkdown,
    {
      components: {
        h2: ({ children }) => {
          const id = headingIds[headingIndex];
          headingIndex += 1;
          return createElement('h2', { id }, children);
        },
      },
    },
    markdown,
  );
}
