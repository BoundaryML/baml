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