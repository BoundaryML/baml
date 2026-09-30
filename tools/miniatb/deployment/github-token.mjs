import { sign } from 'node:crypto';
import { pathToFileURL } from 'node:url';

export function publishRepository(value) {
  if (typeof value !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9-]{0,38}\/[A-Za-z0-9][A-Za-z0-9_.-]{0,99}$/.test(value)
      || value.split('/')[0].toLowerCase() === 'boundaryml') throw new Error('Configure MINIATB_PUBLISH_REPO as a dedicated fork outside BoundaryML');
  return value;
}

export async function installationToken(mode, env = process.env, request = fetch) {
  if (!['read', 'publish', 'push'].includes(mode)) throw new Error('Invalid GitHub token mode');
  const repository = mode === 'push' ? publishRepository(env.MINIATB_PUBLISH_REPO) : 'BoundaryML/baml';
  const client = env.BAMMY_GITHUB_APP_CLIENT_ID;
  const key = env.BAMMY_GITHUB_APP_PRIVATE_KEY;
  if (!client || !key) throw new Error('Bammy GitHub App credentials missing');
  const encode = (value) => Buffer.from(JSON.stringify(value)).toString('base64url');
  const now = Math.floor(Date.now() / 1000);
  const payload = `${encode({ alg: 'RS256', typ: 'JWT' })}.${encode({ iat: now - 60, exp: now + 540, iss: client })}`;
  let signature;
  try {
    signature = sign('RSA-SHA256', Buffer.from(payload), key.replaceAll('\\n', '\n')).toString('base64url');
  } catch {
    throw new Error('Invalid Bammy GitHub App private key');
  }
  const jwt = `${payload}.${signature}`;
  async function api(path, body) {
    const response = await request(`https://api.github.com${path}`, {
      method: body ? 'POST' : 'GET',
      redirect: 'error',
      signal: AbortSignal.timeout(20000),
      headers: {
        Authorization: `Bearer ${jwt}`,
        Accept: 'application/vnd.github+json',
        'X-GitHub-Api-Version': '2022-11-28',
        'User-Agent': 'miniatb',
        'Content-Type': 'application/json',
      },
      body: body ? JSON.stringify(body) : undefined,
    });
    if (!response.ok) throw new Error(`GitHub App authentication failed (HTTP ${response.status})`);
    return response.json();
  }
  const installation = await api(`/repos/${repository}/installation`);
  if (!Number.isSafeInteger(installation.id) || installation.id <= 0) throw new Error('Invalid GitHub installation');
  const result = await api(`/app/installations/${installation.id}/access_tokens`, {
    repositories: [repository.split('/')[1]],
    permissions: mode === 'read' ? { issues: 'read' } : mode === 'push' ? { contents: 'write' } : { contents: 'read', pull_requests: 'write' },
  });
  if (typeof result.token !== 'string' || !result.token || /\s/.test(result.token)) throw new Error('Invalid GitHub installation token');
  return result.token;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  installationToken(process.argv[2]).then((token) => process.stdout.write(token)).catch((error) => {
    // Never print provider response bodies or credentials.
    const message = error.message.startsWith('GitHub App authentication failed') ? error.message : 'GitHub App authentication failed';
    process.stderr.write(`${message}\n`);
    process.exitCode = 1;
  });
}
