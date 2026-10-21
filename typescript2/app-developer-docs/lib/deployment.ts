export function shouldIndexDeployment(
  vercelEnvironment = process.env.VERCEL_ENV,
): boolean {
  return vercelEnvironment !== 'preview' && vercelEnvironment !== 'development';
}

export function robotsDisallowEntireSite(robots: string): boolean {
  return robots
    .split(/\r?\n/)
    .some((line) => line.trim().toLowerCase() === 'disallow: /');
}
