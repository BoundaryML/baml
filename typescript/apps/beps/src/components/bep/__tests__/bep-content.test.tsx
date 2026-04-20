import { render } from '@testing-library/react';
import { BepContent } from '../bep-content';

describe('BepContent', () => {
  it('should render br tags in table cells', () => {
    const markdown = `
| Feature | Description |
| --- | --- |
| Line breaks | First line<br>Second line<br>Third line |
| Single line | Just one line |
`;

    const { container } = render(<BepContent content={markdown} />);
    
    // Check if table is rendered
    const table = container.querySelector('table');
    expect(table).toBeInTheDocument();
    
    // Check if br tags are rendered as actual line breaks
    const cells = container.querySelectorAll('td');
    const cellWithBreaks = Array.from(cells).find(cell => 
      cell.textContent?.includes('First line')
    );
    
    expect(cellWithBreaks).toBeInTheDocument();
    
    // Check that br elements exist in the cell
    const brTags = cellWithBreaks?.querySelectorAll('br');
    expect(brTags?.length).toBeGreaterThan(0);
  });

  it('should render tables without br tags normally', () => {
    const markdown = `
| Column 1 | Column 2 |
| --- | --- |
| Value 1 | Value 2 |
`;

    const { container } = render(<BepContent content={markdown} />);
    
    const table = container.querySelector('table');
    expect(table).toBeInTheDocument();
    
    const cells = container.querySelectorAll('td');
    expect(cells.length).toBe(2);
  });

  it('should handle complex tables with code and br tags', () => {
    const markdown = `
| API Method | Parameters |
| --- | --- |
| \`createUser\` | name: string<br>email: string<br>age: number |
`;

    const { container } = render(<BepContent content={markdown} />);
    
    const table = container.querySelector('table');
    expect(table).toBeInTheDocument();
    
    // Check for code element
    const code = container.querySelector('code');
    expect(code).toBeInTheDocument();
    expect(code?.textContent).toBe('createUser');
    
    // Check for br tags
    const brTags = container.querySelectorAll('br');
    expect(brTags.length).toBeGreaterThan(0);
  });
});
