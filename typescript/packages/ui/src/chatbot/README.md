These are components synthesized using Claude, with the ultimate purpose of
reusing them in ask-baml-client. To actually integrate these into
ask-baml-client, the underlying `Message` structure used in these components
needs to be aligned with the ask-baml-client message types.

These components were synthesized with Claude by extracting them from
github.com/vercel/ai-chatbot starting with this prompt:

> This is a Next.js app that presents an AI Chatbot interface. Investigate this
> and understand how the UI components are laid out. My ultimate goal is to
> extract the chat message components and reuse them in another project, with its
> own logic for retries, message sending, feedback, etc, but largely the same
> styling around messages, retry buttons, reactions, markdown, etc. I do not want
> to preserve any of the auth logic in this project, nor do I want to add
> dependencies on ai-sdk.

It was split out into a standalone repository, fixed in that repository by telling
Claude to iterate with `pnpm tsc`.

After that, it was pulled into here by copying `components/*.tsx` and `lib/utils.ts`
into this directory (`ui/src/chatbot/`) and then instructing Claude to fix it with
this prompt:

> typescript/packages/ui/src/chatbot has been copied from another repo. fix the
> code according to `pnpm tsc` to compile in packages/ui. this is the structure
> it came from:
>
> components
> ├── code-block.tsx
> ├── icons.tsx
> ├── index.ts
> ├── markdown.tsx
> ├── message-actions.tsx
> ├── message-reasoning.tsx
> ├── message.tsx
> ├── messages.tsx
> └── ui
>     ├── button.tsx
>     └── tooltip.tsx
> lib
> └── utils.ts