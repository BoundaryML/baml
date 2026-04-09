import { BamlRuntime, Collector } from '@boundaryml/baml';

interface TourReplRequest {
  code: string;
  functionName: string;
  args: Record<string, unknown>;
}

interface RequestShape {
  method: string;
  url: string;
  headers: Record<string, string>;
  body: string;
}

function jsonResponse(data: unknown, status = 200): Response {
  return new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

function toMessage(error: unknown): string {
  if (error instanceof Error && error.message) {
    return error.message;
  }
  if (error && typeof error === 'object') {
    const record = error as Record<string, unknown>;
    if (typeof record.detailed_message === 'string') {
      return record.detailed_message;
    }
    if (typeof record.message === 'string') {
      return record.message;
    }
  }
  try {
    return JSON.stringify(error, null, 2);
  } catch {
    return String(error);
  }
}

function toEnvRecord(env: Record<string, string | undefined>): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [key, value] of Object.entries(env)) {
    if (value !== undefined) {
      out[key] = value;
    }
  }
  return out;
}

function normalizeHeaders(input: unknown): Record<string, string> {
  if (!input || typeof input !== 'object') {
    return {};
  }

  return Object.fromEntries(
    Object.entries(input as Record<string, unknown>).map(([key, value]) => [key, String(value)])
  );
}

function toRequestShape(httpRequest: any): RequestShape {
  return {
    method: String(httpRequest?.method ?? 'POST'),
    url: String(httpRequest?.url ?? ''),
    headers: normalizeHeaders(httpRequest?.headers),
    body: String(httpRequest?.body?.text?.() ?? ''),
  };
}

function flattenPromptContent(content: unknown): string {
  if (typeof content === 'string') {
    return content;
  }

  if (Array.isArray(content)) {
    return content
      .map(item => flattenPromptContent(item))
      .filter(Boolean)
      .join('\n');
  }

  if (content && typeof content === 'object') {
    const record = content as Record<string, unknown>;

    if (typeof record.text === 'string') {
      return record.text;
    }

    if ('content' in record) {
      return flattenPromptContent(record.content);
    }

    return JSON.stringify(content);
  }

  return '';
}

function extractPromptPreview(bodyText: string): string | null {
  try {
    const parsed = JSON.parse(bodyText) as Record<string, unknown>;
    const sections: string[] = [];

    if (typeof parsed.system === 'string' && parsed.system.trim()) {
      sections.push(`[system]\n${parsed.system}`);
    }

    if (Array.isArray(parsed.messages)) {
      for (const message of parsed.messages) {
        if (!message || typeof message !== 'object') {
          continue;
        }
        const msg = message as Record<string, unknown>;
        const role = typeof msg.role === 'string' ? msg.role : 'message';
        const text = flattenPromptContent(msg.content).trim();
        if (text) {
          sections.push(`[${role}]\n${text}`);
        }
      }
    }

    if (typeof parsed.prompt === 'string' && parsed.prompt.trim()) {
      sections.push(`[prompt]\n${parsed.prompt}`);
    }

    if (typeof parsed.input === 'string' && parsed.input.trim()) {
      sections.push(`[input]\n${parsed.input}`);
    }

    const preview = sections.join('\n\n').trim();
    return preview.length > 0 ? preview : null;
  } catch {
    return null;
  }
}

function extractExecutionMetadata(collector: Collector) {
  const log: any = collector.last;
  const selectedCall = log?.selectedCall as any;
  const usage = selectedCall?.usage ?? log?.usage;
  const timing = selectedCall?.timing ?? log?.timing;

  return {
    provider: selectedCall?.provider ?? null,
    clientName: selectedCall?.clientName ?? null,
    rawOutput: log?.rawLlmResponse ?? null,
    usage: usage
      ? {
          inputTokens: usage.inputTokens ?? null,
          outputTokens: usage.outputTokens ?? null,
          cachedInputTokens: usage.cachedInputTokens ?? null,
        }
      : null,
    timingMs: typeof timing?.durationMs === 'number' ? timing.durationMs : null,
  };
}

export async function handleTourReplRequest(req: Request): Promise<Response> {
  if (req.method !== 'POST') {
    return new Response('Method not allowed', { status: 405 });
  }

  let payload: TourReplRequest;
  try {
    payload = (await req.json()) as TourReplRequest;
  } catch {
    return jsonResponse({ ok: false, stage: 'validation', error: 'Invalid JSON body' }, 400);
  }

  const { code, functionName, args } = payload ?? {};

  if (typeof code !== 'string' || code.trim().length === 0) {
    return jsonResponse({ ok: false, stage: 'validation', error: 'code is required' }, 400);
  }

  if (code.length > 80_000) {
    return jsonResponse({ ok: false, stage: 'validation', error: 'code is too large (max 80k chars)' }, 400);
  }

  if (typeof functionName !== 'string' || !/^[A-Za-z_][A-Za-z0-9_]*$/.test(functionName)) {
    return jsonResponse({ ok: false, stage: 'validation', error: 'functionName must be a valid identifier' }, 400);
  }

  if (!args || typeof args !== 'object' || Array.isArray(args)) {
    return jsonResponse({ ok: false, stage: 'validation', error: 'args must be a JSON object' }, 400);
  }

  const envVars = toEnvRecord(process.env);
  const files = {
    'main.baml': code,
  };

  let runtime: BamlRuntime;
  try {
    runtime = BamlRuntime.fromFiles('/tour/repl', files, envVars);
  } catch (error) {
    return jsonResponse(
      {
        ok: false,
        stage: 'compile',
        error: toMessage(error),
        promptPreview: null,
        request: null,
      },
      400
    );
  }

  let requestShape: RequestShape | null = null;
  let promptPreview: string | null = null;

  try {
    const request = await runtime.buildRequest(
      functionName,
      args,
      runtime.createContextManager(),
      null,
      null,
      false,
      envVars
    );

    requestShape = toRequestShape(request);
    promptPreview = extractPromptPreview(requestShape.body);
  } catch (error) {
    return jsonResponse(
      {
        ok: false,
        stage: 'request',
        error: toMessage(error),
        promptPreview: null,
        request: null,
      },
      400
    );
  }

  const collector = new Collector('tour-repl');

  try {
    const result = await runtime.callFunction(
      functionName,
      args,
      runtime.createContextManager(),
      null,
      null,
      [collector],
      {},
      envVars
    );

    const output = result.parsed(false);
    const metadata = extractExecutionMetadata(collector);

    return jsonResponse({
      ok: true,
      promptPreview,
      request: requestShape,
      output,
      rawOutput: metadata.rawOutput,
      provider: metadata.provider,
      clientName: metadata.clientName,
      timingMs: metadata.timingMs,
      usage: metadata.usage,
    });
  } catch (error) {
    return jsonResponse(
      {
        ok: false,
        stage: 'execution',
        error: toMessage(error),
        promptPreview,
        request: requestShape,
      },
      400
    );
  }
}
