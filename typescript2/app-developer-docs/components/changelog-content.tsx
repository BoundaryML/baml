import ReactMarkdown from 'react-markdown';

export function ChangelogContent({
  headingIds,
  markdown,
}: {
  headingIds: string[];
  markdown: string;
}) {
  let headingIndex = 0;
  return (
    <ReactMarkdown
      components={{
        h2: ({ children }) => {
          const id = headingIds[headingIndex];
          headingIndex += 1;
          if (!id) throw new Error('Changelog heading has no canonical ID');
          return <h2 id={id}>{children}</h2>;
        },
      }}
    >
      {markdown}
    </ReactMarkdown>
  );
}
