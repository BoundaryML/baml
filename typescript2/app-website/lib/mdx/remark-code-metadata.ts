import { type Code, type Root } from 'mdast';
import { type Plugin } from 'unified';
import { visit } from 'unist-util-visit';

/**
 * A remark plugin that extracts metadata from code blocks and stores it
 * in a way that survives the MDX transformation process.
 */
const remarkCodeMetadata: Plugin<[], Root> = () => {
  return (tree) => {
    visit(tree, 'code', (node: Code) => {
      if (!node.meta) return;

      // Parse the meta string to extract filename
      const filenameMatch = node.meta.match(/filename="([^"]*)"/);
      if (filenameMatch) {
        // Store the filename in the data property which will be preserved
        node.data = node.data || {};
        node.data.hProperties = node.data.hProperties || {};
        node.data.hProperties.filename = filenameMatch[1];
      }
    });
  };
};

export default remarkCodeMetadata;
