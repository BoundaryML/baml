/** Every language tab has a local logo; add new languages here before authoring. */
export const languageTabDefinitions = {
  BAML: { logo: '/baml-logo.png', monochrome: false },
  'Effect.ts': { logo: '/languages/effect.svg', monochrome: true },
  Rust: { logo: '/languages/rust.svg', monochrome: true },
  TypeScript: { logo: '/languages/typescript.svg', monochrome: false },
} as const;

export type LanguageTabName = keyof typeof languageTabDefinitions;

export function languageTabDefinition(language: LanguageTabName) {
  const definition = languageTabDefinitions[language];
  if (!definition) {
    throw new Error(
      `Add a logo for language tab "${language}" in lib/content/language-tabs.ts`,
    );
  }
  return definition;
}
