import type { QueryRequest } from './types';

// Define the placeholder queries with optional inspect notes
export const PLACEHOLDER_QUERIES: {
  input: QueryRequest;
  inspectNotes?: string;
}[] = [
  {
    input: {
      query:
        'can I load enums or classes from a saved state, (after defining dynamically previously, then saving)',
      prev_messages: [],
    },
  },
  {
    input: {
      query:
        'cal I load enums from a class not created in baml? (for instance saved state of dynamic types)',
      prev_messages: [],
    },
  },
  {
    input: {
      query: 'Can I bring my own LLM client?',
      prev_messages: [],
    },
  },
  {
    input: {
      query: 'How do I see the prompt that rendered in the response',
      prev_messages: [],
    },
  },
  {
    input: {
      query: 'building a test to incorporate a test image file',
      prev_messages: [],
    },
  },
  {
    input: {
      query: 'Can I use Excel sheets as an input',
      prev_messages: [],
    },
  },
  {
    input: {
      query: 'hi how do i do this',
      prev_messages: [],
    },
  },
  {
    input: {
      query: 'How can I type-hint a list of 3-lenght tuples of strings?',
      prev_messages: [],
    },
  },
  {
    input: {
      query: 'i dont understand why this is required. really. Give an example',
      prev_messages: [],
    },
  },
  {
    input: {
      query:
        "I'm not a developer or software engineer but can i still learn baml?",
      prev_messages: [],
    },
  },
  {
    input: {
      query: 'what do i have to know to learn baml?',
      prev_messages: [],
    },
  },
  {
    input: {
      query: 'How can I control retries and fallback?',
      prev_messages: [],
    },
  },
  {
    input: {
      query:
        'Help me understand this code:\n\n\ndef _pick_best_categories(text: str, categories: list[Category]) -> list[Category]:\n    tb = TypeBuilder()\n    for k in categories:\n        val = tb.Category.add_value(k.name)\n        val.description(k.llm_description)\n    selected_categories = b.PickBestCategories(text, count=3, baml_options={ "tb": tb })\n    return [category for category in categories if category.name in selected_categories]',
      prev_messages: [],
    },
  },
  {
    input: {
      query:
        'i am using provider "openai-generic" to tlak to ollama what options am i allowed ot pass?',
      prev_messages: [],
    },
  },
  {
    input: {
      query: 'can you give an example of how the type alias is used?',
      prev_messages: [],
    },
    inspectNotes:
      'answer should talk about the `@alias` feature of BAML, but often treats' +
      'this as a "type alias" in the generic sense of the phrase',
  },
  {
    input: {
      query: 'can you make an alias dynamically for an existing enum using tb?',
      prev_messages: [],
    },
    inspectNotes: 'answer should reference TypeBuilder, not baml code',
  },
  {
    input: {
      query: 'Is there a retry after a BamlValidationError?',
      prev_messages: [],
    },
  },
];
