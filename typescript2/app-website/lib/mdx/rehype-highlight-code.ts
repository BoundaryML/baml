import bamlGrammar from '@b/pkg-grammar/baml.tmLanguage.json' with {
  type: 'json',
};
import type { Element, Root } from 'hast';
import {
  type BundledLanguage,
  bundledLanguages,
  getSingletonHighlighter,
} from 'shiki';
import type { Plugin } from 'unified';
import { visit } from 'unist-util-visit';

/** Highlight fences at build time, retaining the pre/code structure used by CodeBlocks. */
const rehypeHighlightCode: Plugin<[], Root> = () => async (tree) => {
  const blocks: {
    pre: Element;
    code: Element;
    language: string;
    source: string;
  }[] = [];

  visit(tree, 'element', (pre) => {
    if (pre.tagName !== 'pre' || pre.children.length !== 1) return;
    const code = pre.children[0];
    if (code?.type !== 'element' || code.tagName !== 'code') return;
    const classes = code.properties.className;
    const language = Array.isArray(classes)
      ? classes
          .find(
            (name) => typeof name === 'string' && name.startsWith('language-'),
          )
          ?.toString()
          .slice(9)
      : undefined;
    if (
      !language ||
      (language !== 'baml' && !Object.hasOwn(bundledLanguages, language))
    )
      return;
    // Leave hand-authored JSX and already-highlighted code alone.
    if (!code.children.every((child) => child.type === 'text')) return;
    const source = code.children.map((child) => child.value).join('');
    blocks.push({ code, language, pre, source });
  });

  if (blocks.length === 0) return;
  const languages = [...new Set(blocks.map((block) => block.language))];
  const highlighter = await getSingletonHighlighter({
    langs: languages.map((language) =>
      language === 'baml'
        ? bamlGrammar
        : bundledLanguages[language as BundledLanguage],
    ),
    themes: ['github-dark'],
  });

  for (const { pre, code, language, source } of blocks) {
    const highlighted = highlighter.codeToHast(source, {
      lang: language,
      theme: 'github-dark',
    });
    const highlightedPre = highlighted.children[0];
    if (highlightedPre?.type !== 'element') continue;
    const highlightedCode = highlightedPre.children[0];
    if (highlightedCode?.type !== 'element') continue;

    // Keep filename metadata, language classes, and raw text for tab labels/copying.
    pre.properties = { ...highlightedPre.properties, ...pre.properties };
    code.children = highlightedCode.children;
  }
};

export default rehypeHighlightCode;
