# Demo: BR Tag Support in BEPS Tables

## Problem Statement
Previously, there was no way to add line breaks inside BEPS markdown table cells. This made it difficult to format tables with multi-line content, such as:
- API parameter lists
- Multiple status updates
- Step-by-step instructions
- Comparison features

## Solution
Added support for HTML `<br>` tags inside markdown table cells.

---

## Visual Comparison

### ❌ Before (Without BR Support)

When you tried to add line breaks in table cells:

```markdown
| Method | Parameters |
| --- | --- |
| createUser | name: string
email: string
age: number |
```

**Result**: All parameters would appear on one long line, or the table would break.

Or if you tried using `<br>`:

```markdown
| Method | Parameters |
| --- | --- |
| createUser | name: string<br>email: string<br>age: number |
```

**Result**: The literal text `<br>` would appear in the cell.

---

### ✅ After (With BR Support)

Now you can use `<br>` tags:

```markdown
| Method | Parameters |
| --- | --- |
| createUser | name: string<br>email: string<br>age: number |
```

**Result**: Each parameter appears on its own line within the table cell!

| Method | Parameters |
| --- | --- |
| createUser | name: string<br>email: string<br>age: number |
| deleteUser | id: number |
| updateUser | id: number<br>updates: Partial\<User\> |

---

## Real-World Use Cases

### 1. API Documentation

```markdown
| Endpoint | Request Body | Response |
| --- | --- | --- |
| POST /users | {<br>&nbsp;&nbsp;"name": "string",<br>&nbsp;&nbsp;"email": "string",<br>&nbsp;&nbsp;"role": "string"<br>} | User object<br>Status: 201 |
```

### 2. Feature Comparison

```markdown
| Feature | Free Plan | Pro Plan |
| --- | --- | --- |
| Storage | 10 GB<br>Max file size: 100 MB | Unlimited<br>Max file size: 5 GB |
| Support | Email only<br>Response time: 48h | Email + Chat + Phone<br>Response time: 2h<br>Dedicated account manager |
```

### 3. Status Tracking

```markdown
| Task | Status | Timeline |
| --- | --- | --- |
| Backend API | ✅ Complete | Started: Jan 1<br>Reviewed: Jan 5<br>Merged: Jan 6<br>Deployed: Jan 7 |
| Frontend UI | 🔄 In Progress | Started: Jan 8<br>Current: Jan 15<br>ETA: Jan 20 |
```

### 4. Configuration Options

```markdown
| Setting | Development | Production |
| --- | --- | --- |
| Database | localhost:5432<br>User: dev<br>Pool size: 5 | db.example.com:5432<br>User: app<br>Pool size: 20<br>SSL: enabled |
```

---

## Technical Implementation

### What Changed

**File: `src/components/bep/bep-content.tsx`**
```typescript
// Before
import remarkGfm from "remark-gfm";

<MarkdownHooks remarkPlugins={[remarkGfm]} components={components}>

// After  
import remarkGfm from "remark-gfm";
import rehypeRaw from "rehype-raw";

<MarkdownHooks 
  remarkPlugins={[remarkGfm]} 
  rehypePlugins={[rehypeRaw]}  // <-- Added this
  components={components}
>
```

**File: `src/components/comments/comment-sidebar.tsx`**
```typescript
// Same change applied to CommentText component
```

### Why This Works

1. `react-markdown` uses a plugin architecture
2. `remark-gfm` adds GitHub Flavored Markdown (including tables)
3. `rehype-raw` allows raw HTML tags to be processed
4. Together, they enable `<br>` tags to render as actual line breaks

---

## Browser Compatibility

✅ Works in all modern browsers:
- Chrome/Edge 90+
- Firefox 88+
- Safari 14+

The `<br>` tag is a standard HTML element with universal support.

---

## Best Practices

### ✅ Good Usage

```markdown
| Column | Content |
| --- | --- |
| Short lists | Item 1<br>Item 2<br>Item 3 |
| Parameters | param1: type1<br>param2: type2 |
```

### ⚠️ Avoid

```markdown
| Column | Content |
| --- | --- |
| Too many items | Line 1<br>Line 2<br>Line 3<br>Line 4<br>Line 5<br>Line 6<br>Line 7 |
```

If you have too many items, consider:
1. Using a nested list instead
2. Breaking into multiple rows
3. Creating a separate section

---

## Accessibility

✅ The `<br>` tag is semantically appropriate for line breaks and works well with screen readers.

---

## Performance

No performance impact - `<br>` tags are native HTML elements that browsers render efficiently.

---

## Next Steps

1. ✅ Feature implemented
2. ✅ Tests added
3. ✅ Documentation created
4. 📋 User guide update (future)
5. 📋 Add to BEPS tips/shortcuts page (future)

---

## Questions?

- Check the [CHANGELOG_BR_SUPPORT.md](./CHANGELOG_BR_SUPPORT.md) for technical details
- Review [TEST_BR_TAGS.md](./TEST_BR_TAGS.md) for comprehensive test cases
- See PR [#3384](https://github.com/BoundaryML/baml/pull/3384) for discussion
