// The site's absolute URL, for metadataBase (the share-image URLs), robots.txt and the sitemap.
// boundaryml.com is the Boundary site (typescript2/app-website), so this one takes its own
// deployment's production domain, the same way app-website resolves its own.
export const SITE_URL =
  process.env.NEXT_PUBLIC_BASE_URL ??
  (process.env.VERCEL_PROJECT_PRODUCTION_URL
    ? `https://${process.env.VERCEL_PROJECT_PRODUCTION_URL}`
    : 'http://localhost:3000');
