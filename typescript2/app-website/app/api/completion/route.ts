import { type NextRequest, NextResponse } from 'next/server';
import { b } from '@/baml_client';

// GET route handler
export async function POST(req: NextRequest) {
  console.log('starting completion');
  const { prefix, suffix, language } = await req.json();
  let truncatedPrefix = prefix;
  const charsToKeep = 3000;
  if (truncatedPrefix.length > charsToKeep) {
    truncatedPrefix = truncatedPrefix.substring(0, charsToKeep);
  }
  console.log('truncatedPrefix', truncatedPrefix);
  console.log('suffix', suffix);

  let text;
  if (language === 'py' || language === 'python') {
    text = await b.CompletionPython(prefix ?? '', suffix ?? '', language);
  } else if (language === 'ts' || language === 'typescript') {
    text = await b.CompletionTypescript(prefix ?? '', suffix ?? '', language);
  } else {
    text = await b.Completion(prefix ?? '', suffix ?? '', language);
  }

  // Only accept completion up until where suffix starts
  if (text && suffix) {
    const suffixStart = suffix.trim().slice(0, 5); // Take first 5 chars of suffix
    if (suffixStart) {
      const suffixIndex = text.indexOf(suffixStart);
      if (suffixIndex !== -1) {
        console.log('truncating text at suffix start', suffixIndex);
        text = text.slice(0, suffixIndex);
      }
    }
  }

  // If text is empty or only whitespace after filtering, treat as no completion
  if (!text?.trim()) {
    text = '';
  }

  console.log('DID CHAT COMPLETION', text);
  console.log('DID CHAT COMPLETION' + text?.includes('N/A') ? 'N/A' : '');
  const result = text?.includes('N/A') ? '' : text;
  // strip out the ---
  const result2 = result.replace(/---/g, '');
  return NextResponse.json({ prediction: result2 });
}
