import 'server-only';
import {
  createCipheriv,
  createDecipheriv,
  createHash,
  randomBytes,
} from 'node:crypto';
import { cookies } from 'next/headers';

const SESSION = '__Host-babysit-session';
export const STATE_COOKIE = '__Host-babysit-oauth';
export const cookieOptions = {
  httpOnly: true,
  path: '/',
  sameSite: 'lax' as const,
  secure: true,
};

export function siteOrigin(): string {
  const url = new URL(process.env.FEEDBACK_SITE_URL ?? '');
  if (
    url.protocol !== 'https:' ||
    url.username ||
    url.password ||
    url.pathname !== '/' ||
    url.search ||
    url.hash
  ) {
    throw new Error('FEEDBACK_SITE_URL must be an HTTPS origin');
  }
  return url.origin;
}
function key(): Buffer {
  const value = process.env.FEEDBACK_APPROVAL_SESSION_KEY ?? '';
  if (!/^[a-f0-9]{64}$/i.test(value))
    throw new Error('Approval session key is not configured');
  return Buffer.from(value, 'hex');
}
export function seal(value: object): string {
  const iv = randomBytes(12);
  const cipher = createCipheriv('aes-256-gcm', key(), iv);
  const encrypted = Buffer.concat([
    cipher.update(JSON.stringify(value), 'utf8'),
    cipher.final(),
  ]);
  return Buffer.concat([iv, cipher.getAuthTag(), encrypted]).toString(
    'base64url',
  );
}
export function unseal(value: string): Record<string, unknown> | null {
  try {
    if (value.length > 6000) return null;
    const data = Buffer.from(value, 'base64url');
    const decipher = createDecipheriv(
      'aes-256-gcm',
      key(),
      data.subarray(0, 12),
    );
    decipher.setAuthTag(data.subarray(12, 28));
    const obj = JSON.parse(
      Buffer.concat([
        decipher.update(data.subarray(28)),
        decipher.final(),
      ]).toString(),
    );
    return typeof obj.expires === 'number' && obj.expires > Date.now()
      ? obj
      : null;
  } catch {
    return null;
  }
}
export function nonce(): string {
  return randomBytes(32).toString('base64url');
}
export function challenge(verifier: string): string {
  return createHash('sha256').update(verifier).digest('base64url');
}
export function proposalPath(id: string): string {
  if (!/^[a-zA-Z0-9-]{1,80}$/.test(id)) throw new Error('Invalid proposal ID');
  return `/proposals/${id}`;
}
async function github(
  path: string,
  token: string,
): Promise<Record<string, unknown>> {
  const response = await fetch(`https://api.github.com${path}`, {
    cache: 'no-store',
    headers: {
      Accept: 'application/vnd.github+json',
      Authorization: `Bearer ${token}`,
      'X-GitHub-Api-Version': '2022-11-28',
    },
    redirect: 'error',
    signal: AbortSignal.timeout(10000),
  });
  if (!response.ok)
    throw new Error('GitHub authorization could not be verified');
  return response.json();
}
export async function maintainer(token: string): Promise<string | null> {
  const [user, repo] = await Promise.all([
    github('/user', token),
    github('/repos/BoundaryML/baml', token),
  ]);
  const permissions = repo.permissions as Record<string, boolean> | undefined;
  return typeof user.login === 'string' &&
    (permissions?.admin === true || permissions?.maintain === true)
    ? user.login
    : null;
}
export async function currentApprover(): Promise<string | null> {
  const session = unseal((await cookies()).get(SESSION)?.value ?? '');
  if (typeof session?.token !== 'string') return null;
  // Revalidate membership at every read/approval, so revocation takes effect.
  return maintainer(session.token);
}
export async function setSession(token: string): Promise<void> {
  (await cookies()).set(
    SESSION,
    seal({ expires: Date.now() + 3600000, token }),
    { ...cookieOptions, maxAge: 3600 },
  );
}

export function issuePath(id: string): string {
  if (!/^[a-zA-Z0-9-]{1,100}$/.test(id)) throw new Error('Invalid issue ID');
  return `/issues/${id}`;
}
export async function identity(token: string): Promise<string | null> {
  const user = await github('/user', token);
  return typeof user.login === 'string' &&
    /^[a-zA-Z0-9-]{1,39}$/.test(user.login)
    ? user.login
    : null;
}
export async function currentUser(): Promise<string | null> {
  const session = unseal((await cookies()).get(SESSION)?.value ?? '');
  return typeof session?.token === 'string' ? identity(session.token) : null;
}
