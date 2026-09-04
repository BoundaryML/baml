export function shouldIndexDeployment(
  vercelEnvironment = process.env.VERCEL_ENV,
): boolean {
  return vercelEnvironment !== 'preview' && vercelEnvironment !== 'development';
}
