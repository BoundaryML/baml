import localFont from 'next/font/local';

// The brand's faces, self-hosted from ./fonts so the build needs no network access. The root layout
// and the 404 page (app/global-not-found.tsx, which renders its own document) both load them.

const geist = localFont({
  display: 'swap',
  src: './fonts/Geist.woff2',
  variable: '--font-geist',
  weight: '100 900',
});
const lexend = localFont({
  display: 'swap',
  src: './fonts/Lexend.woff2',
  variable: '--font-lexend',
  weight: '100 900',
});
const jetbrainsMono = localFont({
  display: 'swap',
  src: './fonts/JetBrainsMono.woff2',
  variable: '--font-jetbrains-mono',
  weight: '100 800',
});
// Geist Pixel Square, the brand's caption face.
const geistPixel = localFont({
  display: 'swap',
  src: './fonts/GeistPixel-Square.otf',
  variable: '--font-geist-pixel',
});

/** The font variables, for the `<html>` element's class. */
export const FONT_VARIABLES = `${geist.variable} ${lexend.variable} ${jetbrainsMono.variable} ${geistPixel.variable}`;
