export const escapeValue = (value: string): string => {
  return value.replace(/[\n\r\t]/g, (match) => {
    switch (match) {
      case '\n':
        return '\\n';
      case '\r':
        return '\\r';
      case '\t':
        return '\\t';
      default:
        return match;
    }
  });
};

export const unescapeValue = (value: string): string => {
  return value.replace(/\\[nrt]/g, (match) => {
    switch (match) {
      case '\\n':
        return '\n';
      case '\\r':
        return '\r';
      case '\\t':
        return '\t';
      default:
        return match;
    }
  });
};

export const REQUIRED_ENV_VAR_UNSET_WARNING =
  'Your BAML clients may fail if this is not set';

export const PLACEHOLDER_VALUES = {
  OPENAI_API_KEY: 'PLACEHOLDER_OPENAI_KEY',
  ANTHROPIC_API_KEY: 'PLACEHOLDER_ANTHROPIC_KEY',
} as const;

export const PLACEHOLDER_ENV_VAR_MESSAGE =
  'This is a placeholder value. Replace with your actual API key when available.';

export const isPlaceholderApiKey = (value: string | undefined): boolean => {
  return value === PLACEHOLDER_VALUES.OPENAI_API_KEY ||
         value === PLACEHOLDER_VALUES.ANTHROPIC_API_KEY;
};