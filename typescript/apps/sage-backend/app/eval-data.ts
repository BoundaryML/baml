import type { QueryRequest } from '@baml/sage-interface';

// Define the placeholder queries with optional inspect notes
export const PLACEHOLDER_QUERIES: {
  input: QueryRequest;
  inspectNotes?: string;
}[] = [
  {
    input: {
      session_id: 'eval-session-1',
      message: {
        role: 'user',
        text: 'can I load enums or classes from a saved state, (after defining dynamically previously, then saving)',
      },
      prev_messages: [],
    },
  },
  {
    input: {
      session_id: 'eval-session-2',
      message: {
        role: 'user',
        text: 'cal I load enums from a class not created in baml? (for instance saved state of dynamic types)',
      },
      prev_messages: [],
    },
  },
  {
    input: {
      session_id: 'eval-session-3',
      message: {
        role: 'user',
        text: 'Can I bring my own LLM client?',
      },
      prev_messages: [],
    },
  },
  {
    input: {
      session_id: 'eval-session-4',
      message: {
        role: 'user',
        text: 'How do I see the prompt that rendered in the response',
      },
      prev_messages: [],
    },
  },
  {
    input: {
      session_id: 'eval-session-5',
      message: {
        role: 'user',
        text: 'building a test to incorporate a test image file',
      },
      prev_messages: [],
    },
  },
  {
    input: {
      session_id: 'eval-session-6',
      message: {
        role: 'user',
        text: 'Can I use Excel sheets as an input',
      },
      prev_messages: [],
    },
  },
  {
    input: {
      session_id: 'eval-session-7',
      message: {
        role: 'user',
        text: 'hi how do i do this',
      },
      prev_messages: [],
    },
  },
  {
    input: {
      session_id: 'eval-session-8',
      message: {
        role: 'user',
        text: 'How can I type-hint a list of 3-lenght tuples of strings?',
      },
      prev_messages: [],
    },
  },
  {
    input: {
      session_id: 'eval-session-9',
      message: {
        role: 'user',
        text: 'i dont understand why this is required. really. Give an example',
      },
      prev_messages: [],
    },
  },
  {
    input: {
      session_id: 'eval-session-10',
      message: {
        role: 'user',
        text: "I'm not a developer or software engineer but can i still learn baml?",
      },
      prev_messages: [],
    },
  },
  {
    input: {
      session_id: 'eval-session-11',
      message: {
        role: 'user',
        text: 'what do i have to know to learn baml?',
      },
      prev_messages: [],
    },
  },
  {
    input: {
      session_id: 'eval-session-12',
      message: {
        role: 'user',
        text: 'How can I control retries and fallback?',
      },
      prev_messages: [],
    },
  },
  {
    input: {
      session_id: 'eval-session-13',
      message: {
        role: 'user',
        text: 'Help me understand this code:\n\n\ndef _pick_best_categories(text: str, categories: list[Category]) -> list[Category]:\n    tb = TypeBuilder()\n    for k in categories:\n        val = tb.Category.add_value(k.name)\n        val.description(k.llm_description)\n    selected_categories = b.PickBestCategories(text, count=3, baml_options={ "tb": tb })\n    return [category for category in categories if category.name in selected_categories]',
      },
      prev_messages: [],
    },
  },
  {
    input: {
      session_id: 'eval-session-14',
      message: {
        role: 'user',
        text: 'i am using provider "openai-generic" to tlak to ollama what options am i allowed ot pass?',
      },
      prev_messages: [],
    },
  },
  {
    input: {
      session_id: 'eval-session-15',
      message: {
        role: 'user',
        text: 'can you give an example of how the type alias is used?',
      },
      prev_messages: [],
    },
    inspectNotes:
      'answer should talk about the `@alias` feature of BAML, but often treats' +
      'this as a "type alias" in the generic sense of the phrase',
  },
  {
    input: {
      session_id: 'eval-session-16',
      message: {
        role: 'user',
        text: 'can you make an alias dynamically for an existing enum using tb?',
      },
      prev_messages: [],
    },
    inspectNotes: 'answer should reference TypeBuilder, not baml code',
  },
  {
    input: {
      session_id: 'eval-session-17',
      message: {
        role: 'user',
        text: 'Is there a retry after a BamlValidationError?',
      },
      prev_messages: [],
    },
  },
];
