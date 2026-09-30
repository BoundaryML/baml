type Event = {
  role: string;
  type: string;
  text: string;
  continuation: boolean;
};

/** Recover the explicit ticket header for older transcripts, never IDs mentioned in prose. */
export function assessmentIssue(
  events: Event[],
): { id: string; title: string } | null {
  const start = events.findIndex(
    (event) => event.role === 'user' && event.type === 'text',
  );
  if (start < 0) return null;
  let prompt = events[start].text;
  for (
    let i = start + 1;
    i < events.length &&
    events[i].continuation &&
    events[i].role === 'user' &&
    events[i].type === 'text';
    i++
  )
    prompt += events[i].text;
  const header = prompt.split('Description:', 1)[0];
  const id = /^\s*Issue ID: ((?:ISSUE|GH)-[A-Za-z0-9_-]+)\s*$/m.exec(
    header,
  )?.[1];
  const title = /^\s*Title: ([^\r\n]+)/m.exec(header)?.[1].trim();
  return id ? { id, title: title || 'View assessed issue' } : null;
}
