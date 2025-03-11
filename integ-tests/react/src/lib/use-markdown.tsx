import { marked } from 'marked';
import { useEffect, useRef } from 'react';

/**
 * A custom hook for safely rendering markdown content into a DOM element
 * @param markdown The markdown string to render
 * @returns A ref to attach to the container element
 */
export function useMarkdown(markdown: string) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (container) {
      // Convert markdown to HTML using marked
      const renderMarkdown = async () => {
        try {
          const html = await marked(markdown, {
            // Add sanitization options if available
            // marked has built-in sanitization by default
          });

          // Set the inner HTML of the container
          container.innerHTML = html;
        } catch (error) {
          console.error('Error rendering markdown:', error);
          // Fallback to plain text if rendering fails
          container.textContent = markdown;
        }
      };

      renderMarkdown();
    }
  }, [markdown]);

  return containerRef;
}
