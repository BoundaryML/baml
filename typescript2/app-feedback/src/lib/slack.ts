// Only link to the configured Slack channel, never a user-supplied redirect.
export function slackChannelUrl(value = process.env.FEEDBACK_SLACK_URL): string | null {
  try {
    const url = new URL(value ?? "");
    if (url.protocol !== "https:" || !/^[a-z0-9-]+\.slack\.com$/.test(url.hostname)
      || url.username || url.password || url.port || url.search || url.hash
      || !/^\/archives\/[CG][A-Z0-9]+\/?$/.test(url.pathname)) return null;
    return url.href;
  } catch { return null; }
}
