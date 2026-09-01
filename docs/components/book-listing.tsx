import type { ReactNode } from 'react';

type Props = {
  caption?: ReactNode;
  children: ReactNode;
  fileName?: string;
  number?: string;
};

export function BookListing({ caption, children, fileName, number }: Props) {
  const id = number ? `listing-${number}` : undefined;

  return (
    <figure className="book-listing not-prose" id={id}>
      {fileName && <div className="book-listing-file">Filename: {fileName}</div>}
      <div className="book-listing-body prose dark:prose-invert">{children}</div>
      {(number || caption) && (
        <figcaption>
          {number && <a href={`#${id}`}>Listing {number}</a>}
          {number && caption ? ': ' : null}
          {caption}
        </figcaption>
      )}
    </figure>
  );
}
