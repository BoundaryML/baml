import palette from '../../../baml_language/crates/baml_lsp/src/rainbow.json';

export const rainbowInitializationOptions = { rustFunctionRainbow: true };

export const rainbowTokenTypes = palette.map(({ token }) => ({
  description: 'Native Rust sigil rainbow color.',
  id: token,
  superType: 'macro',
}));

export const rainbowTokenColors = Object.fromEntries(
  palette.map(({ token, color }) => [`${token}:baml`, color]),
);
