/** @type {import('next-sitemap').IConfig} */

const AGENT_PATHS = ['/agent', '/agent.md', '/llms.txt'];

module.exports = {
  generateRobotsTxt: true,
  robotsTxtOptions: {
    transformRobotsTxt: async (_, robotsTxt) => {
      const withoutHost = robotsTxt.replace(
        `# Host\nHost: ${process.env.SITE_URL}\n\n`,
        '',
      );

      return withoutHost;
    },
    additionalSitemaps: [],
  },
  additionalPaths: async (config) => {
    const now = new Date().toISOString();
    return AGENT_PATHS.map((loc) => ({
      loc,
      changefreq: 'weekly',
      priority: 0.9,
      lastmod: now,
    }));
  },
  siteUrl: process.env.SITE_URL || 'https://boundaryml.com',
};
