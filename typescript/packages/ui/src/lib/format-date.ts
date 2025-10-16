export function formatDate(
  date: Date | string | number,
  options: Intl.DateTimeFormatOptions = {},
) {
  return new Intl.DateTimeFormat('en-US', {
    day: options.day ?? 'numeric',
    month: options.month ?? 'long',
    year: options.year ?? 'numeric',
    ...options,
  }).format(new Date(date));
}
