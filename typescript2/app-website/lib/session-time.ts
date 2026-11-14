export function dateLabel(date: Date, timeZone: string) {
  return new Intl.DateTimeFormat('en-US', {
    day: 'numeric',
    month: 'short',
    timeZone,
    weekday: 'short',
  }).format(date);
}

export function timeLabel(date: Date, timeZone: string) {
  return new Intl.DateTimeFormat('en-US', {
    hour: 'numeric',
    minute: '2-digit',
    timeZone,
    timeZoneName: 'short',
  }).format(date);
}

export function daysUntil(start: Date, now: Date, timeZone: string) {
  const calendarDate = new Intl.DateTimeFormat('sv-SE', {
    day: '2-digit',
    month: '2-digit',
    timeZone,
    year: 'numeric',
  });
  return (
    (Date.parse(calendarDate.format(start)) -
      Date.parse(calendarDate.format(now))) /
    86_400_000
  );
}
