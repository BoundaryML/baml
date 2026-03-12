import { redirect } from "next/navigation";
import { NextRequest } from "next/server";

const PRODUCTION_URL = "https://beps.boundaryml.com";

export async function GET(request: NextRequest) {
  const clientId = process.env.GITHUB_CLIENT_ID;

  if (!clientId) {
    return new Response("GitHub OAuth not configured", { status: 500 });
  }

  const currentUrl = getBaseUrl(request);
  const isProduction = currentUrl === PRODUCTION_URL;
  
  // Always use production callback URL for GitHub OAuth
  // This ensures preview deployments work with a single registered callback
  const redirectUri = `${PRODUCTION_URL}/api/auth/github/callback`;

  // Encode the original URL in state so we can redirect back after auth
  const state = JSON.stringify({
    nonce: crypto.randomUUID(),
    returnTo: isProduction ? null : currentUrl,
  });

  const params = new URLSearchParams({
    client_id: clientId,
    redirect_uri: redirectUri,
    scope: "read:user user:email",
    state: Buffer.from(state).toString("base64"),
  });

  redirect(`https://github.com/login/oauth/authorize?${params.toString()}`);
}

function getBaseUrl(request: NextRequest): string {
  const forwardedHost = request.headers.get("x-forwarded-host");
  const forwardedProto = request.headers.get("x-forwarded-proto") || "https";

  if (forwardedHost) {
    return `${forwardedProto}://${forwardedHost}`;
  }

  const host = request.headers.get("host") || "localhost:3000";
  const protocol = host.includes("localhost") ? "http" : "https";
  return `${protocol}://${host}`;
}
