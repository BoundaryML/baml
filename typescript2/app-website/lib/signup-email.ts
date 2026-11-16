import { SIGNUP_EMAIL_MAX } from './signup';

export function isSignupEmail(value: string) {
  return (
    value.length <= SIGNUP_EMAIL_MAX &&
    /^[^\s@<>",;]+@[^\s@<>",;]+\.[^\s@<>",;]+$/.test(value)
  );
}

export function parseTeammateEmails(value: string) {
  const entries =
    value.match(/(?:"[^"\r\n]*"|[^",;\r\n])+/g)?.flatMap((part) => {
      const named = part.trim().match(/^[^<>]*<([^<>]+)>$/);
      return named ? [named[1]] : part.trim().split(/\s+/);
    }) ?? [];
  return [
    ...new Set(
      entries
        .map((entry) =>
          entry
            .trim()
            .replace(/^mailto:/i, '')
            .toLowerCase(),
        )
        .filter(Boolean),
    ),
  ];
}
