# Test Document: BR Tags in Markdown Tables

This document tests the `<br>` tag support in BEPS markdown tables.

## Test Case 1: Simple Table with Line Breaks

| Feature | Description |
| --- | --- |
| Line breaks | First line<br>Second line<br>Third line |
| Single line | Just one line |

Expected: The "Line breaks" cell should show three lines of text.

## Test Case 2: API Documentation Table

| API Method | Parameters | Return Type |
| --- | --- | --- |
| `createUser` | name: string<br>email: string<br>age: number<br>role: string | User |
| `deleteUser` | id: number | boolean |
| `updateUser` | id: number<br>updates: Partial\<User\> | User |

Expected: Parameter columns should show multiple parameters on separate lines.

## Test Case 3: Status Tracking Table

| Task | Status | Notes |
| --- | --- | --- |
| Implement feature A | ✅ Complete | Merged on Jan 15<br>Tested on Jan 16<br>Deployed on Jan 17 |
| Fix bug B | 🔄 In Progress | Found on Jan 10<br>Fix in progress<br>ETA: Jan 20 |
| Design feature C | 📝 Planning | Initial designs<br>Awaiting feedback<br>Target: Feb 1 |

Expected: Notes column should show timeline events on separate lines.

## Test Case 4: Comparison Table

| Feature | Option A | Option B |
| --- | --- | --- |
| Performance | Fast<br>Low latency<br>High throughput | Moderate<br>Medium latency<br>Good throughput |
| Cost | $100/month<br>$1200/year | $80/month<br>$960/year |
| Support | Email<br>Chat<br>Phone<br>24/7 | Email<br>Chat<br>Business hours |

Expected: Each cell with multiple lines should display them vertically.

## Test Case 5: Mixed Content Table

| Component | Props | Example Usage |
| --- | --- | --- |
| Button | variant: 'primary' \| 'secondary'<br>size: 'sm' \| 'md' \| 'lg'<br>disabled?: boolean | `<Button variant="primary">`<br>`  Click me`<br>`</Button>` |
| Input | type: string<br>placeholder?: string<br>onChange: (value: string) => void | `<Input`<br>`  type="text"`<br>`  placeholder="Enter name"`<br>`/>` |

Expected: Both props and example usage columns should show multiple lines with proper code formatting preserved.

## Test Case 6: Table Without BR Tags (Control)

| Column 1 | Column 2 | Column 3 |
| --- | --- | --- |
| Value 1 | Value 2 | Value 3 |
| Value 4 | Value 5 | Value 6 |

Expected: This table should render normally without any line breaks.

---

## Verification Checklist

When viewing this document in the BEPS app, verify:

- ✅ All `<br>` tags render as actual line breaks
- ✅ Table structure remains intact
- ✅ Code formatting (backticks) works within cells that have `<br>` tags
- ✅ No raw `<br>` text appears (tags should be processed)
- ✅ Tables without `<br>` tags still render correctly
- ✅ The same behavior works in comments sidebar

## Security Note

The `rehype-raw` plugin allows HTML tags to be processed. This is intentional for `<br>` support, but be aware that it could allow other HTML tags as well. In a production environment, consider using `rehype-sanitize` to whitelist only safe tags like `<br>`.
