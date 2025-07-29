import * as React from 'react';

export const highlightText = (text: string, searchTerm: string) => {
  if (!searchTerm) return text;
  const parts = text.split(new RegExp(`(${searchTerm})`, 'gi'));
  return (
    <span>
      {parts.map((part, index) =>
        part.toLowerCase() === searchTerm.toLowerCase() ? (
          <span key={`${part}-${index}`} className="bg-yellow-200 dark:bg-yellow-900">
            {part}
          </span>
        ) : (
          part
        ),
      )}
    </span>
  );
};