/**
 * Standalone test file to verify <br> tag support in react-markdown
 * 
 * To run this test:
 * 1. Create a simple Next.js page that imports this component
 * 2. Or use this in a playground like CodeSandbox
 * 
 * Expected behavior: The table cells should show line breaks where <br> tags are placed
 */

import React from 'react';
import Markdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeRaw from 'rehype-raw';

const testMarkdown = `
## Test: BR Tags in Tables

| Feature | Description |
| --- | --- |
| Line breaks | First line<br>Second line<br>Third line |
| Single line | Just one line |

## Test 2: API Documentation

| Method | Parameters |
| --- | --- |
| \`createUser\` | name: string<br>email: string<br>age: number |
| \`deleteUser\` | id: number |
`;

export default function TestMarkdownBR() {
  return (
    <div style={{ padding: '2rem', maxWidth: '800px', margin: '0 auto' }}>
      <h1>Test: React Markdown with BR Tags</h1>
      
      <div style={{ marginBottom: '2rem' }}>
        <h2>With rehype-raw (should show line breaks)</h2>
        <Markdown 
          remarkPlugins={[remarkGfm]} 
          rehypePlugins={[rehypeRaw]}
        >
          {testMarkdown}
        </Markdown>
      </div>
      
      <div style={{ marginBottom: '2rem' }}>
        <h2>Without rehype-raw (should show literal &lt;br&gt; tags)</h2>
        <Markdown remarkPlugins={[remarkGfm]}>
          {testMarkdown}
        </Markdown>
      </div>
      
      <div style={{ 
        marginTop: '2rem', 
        padding: '1rem', 
        backgroundColor: '#f5f5f5',
        borderRadius: '4px'
      }}>
        <h3>Verification Steps:</h3>
        <ol>
          <li>Check the first table - "Line breaks" cell should show 3 separate lines</li>
          <li>Check the second table - "Parameters" should show multiple lines</li>
          <li>Compare with the "Without rehype-raw" section - it should show literal &lt;br&gt; text</li>
        </ol>
      </div>
    </div>
  );
}
