import { NextRequest, NextResponse } from "next/server";

const PRODUCTION_URL = "https://beps.boundaryml.com";
const ALLOWED_PREVIEW_PATTERNS = [
  /^https:\/\/.*\.vercel\.app$/,
  /^https:\/\/.*-boundaryml\.vercel\.app$/,
  /^http:\/\/localhost:\d+$/,
];

interface GitHubTokenResponse {
  access_token: string;
  token_type: string;
  scope: string;
  error?: string;
  error_description?: string;
}

interface GitHubUser {
  id: number;
  login: string;
  name: string | null;
  email: string | null;
  avatar_url: string;
}

interface GitHubEmail {
  email: string;
  verified: boolean;
  primary: boolean;
  visibility: string | null;
}

interface OAuthState {
  nonce: string;
  returnTo: string | null;
}

function isAllowedReturnUrl(url: string): boolean {
  if (url === PRODUCTION_URL) return true;
  return ALLOWED_PREVIEW_PATTERNS.some((pattern) => pattern.test(url));
}

function parseState(stateParam: string | null): OAuthState | null {
  if (!stateParam) return null;
  try {
    const decoded = Buffer.from(stateParam, "base64").toString("utf-8");
    return JSON.parse(decoded) as OAuthState;
  } catch {
    return null;
  }
}

export async function GET(request: NextRequest) {
  const searchParams = request.nextUrl.searchParams;
  const code = searchParams.get("code");
  const error = searchParams.get("error");
  const stateParam = searchParams.get("state");

  const state = parseState(stateParam);
  const returnTo = state?.returnTo && isAllowedReturnUrl(state.returnTo) 
    ? state.returnTo 
    : null;
  
  const baseUrl = returnTo || PRODUCTION_URL;

  if (error) {
    return NextResponse.redirect(
      new URL(`/login?error=${encodeURIComponent(error)}`, baseUrl)
    );
  }

  if (!code) {
    return NextResponse.redirect(
      new URL("/login?error=no_code", baseUrl)
    );
  }

  const clientId = process.env.GITHUB_CLIENT_ID;
  const clientSecret = process.env.GITHUB_CLIENT_SECRET;

  if (!clientId || !clientSecret) {
    return NextResponse.redirect(
      new URL("/login?error=oauth_not_configured", baseUrl)
    );
  }

  try {
    const tokenResponse = await fetch(
      "https://github.com/login/oauth/access_token",
      {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Accept: "application/json",
        },
        body: JSON.stringify({
          client_id: clientId,
          client_secret: clientSecret,
          code,
        }),
      }
    );

    const tokenData: GitHubTokenResponse = await tokenResponse.json();

    if (tokenData.error) {
      console.error("GitHub token error:", tokenData.error_description);
      return NextResponse.redirect(
        new URL(`/login?error=${encodeURIComponent(tokenData.error)}`, baseUrl)
      );
    }

    const userResponse = await fetch("https://api.github.com/user", {
      headers: {
        Authorization: `Bearer ${tokenData.access_token}`,
        Accept: "application/vnd.github+json",
      },
    });

    if (!userResponse.ok) {
      return NextResponse.redirect(
        new URL("/login?error=failed_to_fetch_user", baseUrl)
      );
    }

    const githubUser: GitHubUser = await userResponse.json();

    // Fetch all emails to check for @boundaryml.com (special account)
    let boundaryEmail: string | undefined;
    try {
      const emailsResponse = await fetch("https://api.github.com/user/emails", {
        headers: {
          Authorization: `Bearer ${tokenData.access_token}`,
          Accept: "application/vnd.github+json",
        },
      });

      if (emailsResponse.ok) {
        const emails: GitHubEmail[] = await emailsResponse.json();
        const boundaryEmailObj = emails.find(
          (e) => e.verified && e.email.endsWith("@boundaryml.com")
        );
        if (boundaryEmailObj) {
          boundaryEmail = boundaryEmailObj.email;
        }
      }
    } catch (error) {
      console.warn("Failed to fetch GitHub emails:", error);
    }

    const userData = {
      githubId: githubUser.id.toString(),
      name: githubUser.name || githubUser.login,
      avatarUrl: githubUser.avatar_url,
      boundaryEmail,
    };

    const encodedUser = encodeURIComponent(JSON.stringify(userData));

    return NextResponse.redirect(
      new URL(`/login?github_user=${encodedUser}`, baseUrl)
    );
  } catch (error) {
    console.error("GitHub OAuth error:", error);
    return NextResponse.redirect(
      new URL("/login?error=oauth_failed", baseUrl)
    );
  }
}
