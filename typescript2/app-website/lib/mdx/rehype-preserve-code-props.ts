import type { Root } from 'hast';
import type { Plugin } from 'unified';
import { visit } from 'unist-util-visit';

interface Options {
  /**
   * The tag name to add the attributes to.
   * @default 'pre'
   */
  tagName?: 'code' | 'pre';
}

const emptyOptions: Options = {};

/**
 * Extract metadata from code blocks and add it as properties while preserving
 * the element structure for syntax highlighting.
 */
const rehypePreserveCodeProps: Plugin<[Options?], Root> = (options) => {
  const settings = options || emptyOptions;
  const tagName = settings.tagName || 'pre';

  return (tree) => {
    visit(tree, 'element', (node, index, parent) => {
      if (node.tagName !== 'code') return;

      // Only process code elements that have meta
      const meta = node.data?.meta;
      if (typeof meta !== 'string' || !meta) return;

      // Parse the meta string to extract filename
      const filenameMatch = meta.match(/filename="([^"]*)"/);
      if (!filenameMatch) return;

      const filename = filenameMatch[1];

      if (tagName === 'code') {
        // Add properties to the code element directly
        node.properties = node.properties || {};
        node.properties.filename = filename;
      } else {
        // Find the parent pre element
        if (!parent || parent.type !== 'element' || parent.tagName !== 'pre')
          return;

        // Add properties to the pre element
        parent.properties = parent.properties || {};
        parent.properties.filename = filename;

        // Preserve any existing className on the code element
        const langClass = Array.isArray(node.properties?.className)
          ? node.properties.className[0]
          : undefined;
        // console.log(node.properties)

        if (langClass) {
          node.data = node.properties || {};
          node.properties = node.properties || {};
          node.properties.className = [langClass];
        }
      }
    });
  };
};

export default rehypePreserveCodeProps;
