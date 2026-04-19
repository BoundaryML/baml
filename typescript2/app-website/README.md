This is a [Next.js](https://nextjs.org/) project bootstrapped with [`create-next-app`](https://github.com/vercel/next.js/tree/canary/packages/create-next-app).

## Getting Started

First, run the development server:

```bash
npm run dev
# or
yarn dev
# or
pnpm dev
# or
bun dev
```

Open [http://localhost:3000](http://localhost:3000) with your browser to see the result.

You can start editing the page by modifying `app/(marketing)/page.tsx`. The page auto-updates as you edit the file.

This project uses [`next/font`](https://nextjs.org/docs/basic-features/font-optimization) to automatically optimize and load Inter, a custom Google Font.

## Learn More

To learn more about Next.js, take a look at the following resources:

- [Next.js Documentation](https://nextjs.org/docs) - learn about Next.js features and API.
- [Learn Next.js](https://nextjs.org/learn) - an interactive Next.js tutorial.

You can check out [the Next.js GitHub repository](https://github.com/vercel/next.js/) - your feedback and contributions are welcome!

## Agent Mode

The site serves a plain-markdown view for LLM agents and crawlers. There are three entry points:

1. **Content negotiation.** Any top-level marketing page returns markdown when the request looks agent-ish. Detection is a pure function in `lib/agent-detect.ts` (UA patterns: curl / wget / httpie / python-requests / node-fetch / Go-http-client / ChatGPT-User / GPTBot / ClaudeBot / Claude-Web / anthropic-ai / PerplexityBot / cohere-ai / Bytespider; Accept-header ranking; query params `?format=md` or `?agent=1`). `middleware.ts` rewrites matching requests to `/agent.md` and sets `Vary: Accept, User-Agent`.
2. **Static routes.** `/agent.md` (Content-Type `text/markdown`) and `/llms.txt` (Content-Type `text/plain`) both stream `content/agent.md` verbatim.
3. **Manual toggle.** The `human` / `agent` pill in the top nav routes to `/agent?from=toggle`, which bypasses middleware and renders the same markdown in a styled monospace layout. The toggle uses `?from=toggle` specifically so agents can't be trapped in the styled view.

### Updating the content

Edit `content/agent.md`. The file is read at request time by all three entry points. `{{TODO}}` placeholders mark claims that should be confirmed against current product docs before publishing.

### Verifying locally

```bash
# Agent view (raw markdown)
curl -sI http://localhost:3000/ | grep -i content-type
curl -s http://localhost:3000/ | head -5

# Human view (HTML)
curl -s -H "Accept: text/html" http://localhost:3000/ | head -5

# Direct static routes
curl -s http://localhost:3000/agent.md | head -5
curl -s http://localhost:3000/llms.txt | head -5

# Query-param override
curl -s "http://localhost:3000/?format=md" | head -5
```

## Deploy on Vercel

The easiest way to deploy your Next.js app is to use the [Vercel Platform](https://vercel.com/new?utm_medium=default-template&filter=next.js&utm_source=create-next-app&utm_campaign=create-next-app-readme) from the creators of Next.js.

Check out our [Next.js deployment documentation](https://nextjs.org/docs/deployment) for more details.
