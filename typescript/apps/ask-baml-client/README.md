See CLAUDE.md for architecture description.

Remaining work items, in priority answer:

[ ] stream chatbot responses to the user
[ ] search bar css is missing dark mode support (maybe integrate tailwind styles?)
[ ] copy chat-sdk.dev’s UI https://chat-sdk.dev/
    [ ] want to allow edit-and-retry, for example
        (this is partially done, see packages/ui/chatbot)
[ ] index headings of each document, to be able to suggest links to
    individual headings when suggesting destinations
[ ] improve the underlying prompt
    [ ] standalone queries like "alias" or "type builder" sometimes result in
        "please provide a complete query" from the chatbot
    [ ] alias → how do i use alias with dynamic types (currently returns the
        wrong answer)
    [ ] what do if user is asking for code debugging help?
[ ] instant search dropdown icons are wrong (needs to get
    propagated through sitemap/pinecone)