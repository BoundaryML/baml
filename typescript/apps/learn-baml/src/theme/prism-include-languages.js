/**
 * Custom Prism language loader for Docusaurus
 * Adds BAML syntax highlighting support
 */

import siteConfig from '@generated/docusaurus.config';

export default function prismIncludeLanguages(PrismObject) {
  const {
    themeConfig: { prism },
  } = siteConfig;

  const { additionalLanguages } = prism;

  // Prism components work on the Prism instance on the window object
  globalThis.Prism = PrismObject;

  // Load additional languages from Prism
  additionalLanguages.forEach((lang) => {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    require(`prismjs/components/prism-${lang}`);
  });

  // Load our custom BAML language
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  require('../prism/prism-baml');

  delete globalThis.Prism;
}
