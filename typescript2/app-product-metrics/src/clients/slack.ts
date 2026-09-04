export interface SlackMrkdwnText {
  text: string;
  type: 'mrkdwn';
}

export type SlackBlock =
  | { text: SlackMrkdwnText; type: 'section' }
  | { elements: SlackMrkdwnText[]; type: 'context' };

export interface WeeklyPost {
  blocks?: SlackBlock[];
  channel: string;
  file?: {
    altText: string;
    bytes: Buffer;
    filename: string;
    title: string;
  };
  text: string;
}

interface SlackPostResponse {
  error?: string;
  file_id?: string;
  ok?: boolean;
  upload_url?: string;
}

async function slackApi(
  token: string,
  method: string,
  body: Record<string, unknown>,
  fetchImpl: typeof fetch,
  encoding: 'form' | 'json' = 'json',
): Promise<SlackPostResponse> {
  const encodedBody =
    encoding === 'form'
      ? new URLSearchParams(
          Object.entries(body).map(([key, value]) => [
            key,
            typeof value === 'string' ? value : JSON.stringify(value),
          ]),
        )
      : JSON.stringify(body);
  const response = await fetchImpl(`https://slack.com/api/${method}`, {
    body: encodedBody,
    headers: {
      Authorization: `Bearer ${token}`,
      'Content-Type':
        encoding === 'form'
          ? 'application/x-www-form-urlencoded; charset=utf-8'
          : 'application/json; charset=utf-8',
    },
    method: 'POST',
    signal: AbortSignal.timeout(30_000),
  });
  const result = (await response.json()) as SlackPostResponse;
  if (!response.ok || !result.ok) {
    throw new Error(
      `Slack ${method} failed: ${result.error ?? response.status}`,
    );
  }
  return result;
}

export async function postToSlack(
  token: string,
  message: WeeklyPost,
  fetchImpl: typeof fetch = fetch,
): Promise<void> {
  if (!message.file) {
    await slackApi(
      token,
      'chat.postMessage',
      {
        channel: message.channel,
        text: message.text,
        ...(message.blocks?.length ? { blocks: message.blocks } : {}),
      },
      fetchImpl,
    );
    return;
  }

  const ticket = await slackApi(
    token,
    'files.getUploadURLExternal',
    {
      alt_txt: message.file.altText,
      filename: message.file.filename,
      length: message.file.bytes.length,
    },
    fetchImpl,
    'form',
  );
  if (!ticket.upload_url || !ticket.file_id) {
    throw new Error('Slack file upload ticket was incomplete');
  }
  const upload = await fetchImpl(ticket.upload_url, {
    body: new Uint8Array(message.file.bytes),
    headers: { 'Content-Type': 'image/png' },
    method: 'POST',
    signal: AbortSignal.timeout(30_000),
  });
  if (!upload.ok) {
    throw new Error(`Slack file upload failed: ${upload.status}`);
  }
  await slackApi(
    token,
    'files.completeUploadExternal',
    {
      channel_id: message.channel,
      files: [{ id: ticket.file_id, title: message.file.title }],
      ...(message.blocks?.length
        ? { blocks: message.blocks }
        : { initial_comment: message.text }),
    },
    fetchImpl,
    'form',
  );
}
