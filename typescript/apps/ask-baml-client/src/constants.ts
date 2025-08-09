const SAGE_URL =
  process.env.NODE_ENV === 'development'
    ? 'http://localhost:4000'
    : 'https://boundary-sage-backend.vercel.app';
export const CHAT_ENDPOINT = `${SAGE_URL}/api/ask-baml/chat`;
export const FEEDBACK_ENDPOINT = `${SAGE_URL}/api/ask-baml/feedback`;

export const ALGOLIA_SEARCH_CREDENTIALS_ENDPOINT =
  'https://docs.boundaryml.com/api/fern-docs/search/v2/key';
export const ALGOLIA_SEARCH_INDEX_NAME = 'fern_docs_search';