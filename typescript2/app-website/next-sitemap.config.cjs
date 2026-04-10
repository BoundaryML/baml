/** @type {import('next-sitemap').IConfig} */

module.exports = {
  generateRobotsTxt: true,
  // https://github.com/iamvishnusankar/next-sitemap/issues/381#issuecomment-1268146841
  // HACK: For removing Host from robots.txt because it's not a valid directive
  robotsTxtOptions: {
    transformRobotsTxt: async (_, robotsTxt) => {
      const withoutHost = robotsTxt.replace(
        `# Host\nHost: ${process.env.SITE_URL}\n\n`,
        '',
      );

      return withoutHost;
    },
  },
  siteUrl: process.env.SITE_URL || 'https://boundaryml.com',
};
