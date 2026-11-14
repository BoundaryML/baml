import bamlGrammar from '@b/pkg-grammar/baml.tmLanguage.json' with {
  type: 'json',
};
import {
  type BundledLanguage,
  bundledLanguages,
  getSingletonHighlighter,
} from 'shiki';
import { LANGS, SAMPLES, SHIKI } from './code';

/** Every code sample on /adopt, highlighted once on the server with the same Shiki setup as the
 *  blog, keyed by the code itself. */
export async function highlightAll() {
  const highlighter = await getSingletonHighlighter({
    langs: [
      bamlGrammar,
      ...Object.values(SHIKI).map(
        (l) => bundledLanguages[l as BundledLanguage],
      ),
    ],
    themes: ['github-dark'],
  });
  const html: Record<string, string> = {};
  const add = (code: string, lang: string) => {
    html[code] = highlighter.codeToHtml(code, { lang, theme: 'github-dark' });
  };
  for (const sample of SAMPLES) {
    add(sample.baml, 'baml');
    for (const [name, code] of Object.entries(sample.calls))
      add(code, SHIKI[name]);
  }
  for (const l of LANGS) add(l.call, SHIKI[l.name]);
  return html;
}
