import { describe, expect, it, vi } from 'vitest';
import type * as VSCode from 'vscode';
import { StdlibDocuments } from './stdlib-documents';

function uri(name: string): VSCode.Uri {
  const text = `baml-stdlib:/baml/${name}.baml`;
  return {
    path: new URL(text).pathname,
    scheme: 'baml-stdlib',
    toString: () => text,
  } as VSCode.Uri;
}

function setup(openUris: VSCode.Uri[] = []) {
  let provider: VSCode.TextDocumentContentProvider;
  const fire = vi.fn();
  const unregister = vi.fn();
  const api = {
    EventEmitter: class {
      event = vi.fn();
      fire = fire;
      dispose = vi.fn();
    },
    workspace: {
      registerTextDocumentContentProvider: (
        _scheme: string,
        value: VSCode.TextDocumentContentProvider,
      ) => {
        provider = value;
        return { dispose: unregister };
      },
      textDocuments: openUris.map((uri) => ({ uri })),
    },
  } as unknown as ConstructorParameters<typeof StdlibDocuments>[0];
  const documents = new StdlibDocuments(api);
  const token = {
    isCancellationRequested: false,
    onCancellationRequested: vi.fn(),
  };
  return {
    documents,
    fire,
    read: (session: string) =>
      provider.provideTextDocumentContent(uri(session), token),
    token,
    unregister,
  };
}

describe('stdlib documents', () => {
  it('forwards source reads and cancellation to the current server', async () => {
    const { documents, read, token } = setup();
    await expect(read('first')).rejects.toThrow('unavailable');
    const first = vi.fn().mockResolvedValue({ content: 'first toolchain' });
    documents.connect(first);
    expect(await read('first')).toBe('first toolchain');
    expect(first).toHaveBeenCalledWith(uri('first').toString(), token);
    documents.dispose();
  });

  it('refreshes open source after reconnect without a stale disposer removing the replacement', async () => {
    const open = uri('first');
    const { documents, read, fire, unregister } = setup([open]);
    const old = documents.connect(async () => ({
      content: 'old source',
    }));
    fire.mockClear();
    const current = documents.connect(async () => ({
      content: 'new source',
    }));
    old.dispose();
    expect(fire).toHaveBeenCalledExactlyOnceWith(open);
    expect(await read('first')).toBe('new source');
    current.dispose();
    await expect(read('first')).rejects.toThrow('unavailable');
    documents.dispose();
    expect(unregister).toHaveBeenCalledOnce();
  });

  it('propagates server errors rather than displaying them as source text', async () => {
    const { documents, read } = setup();
    documents.connect(async () => {
      throw new Error('request cancelled');
    });
    await expect(read('first')).rejects.toThrow('request cancelled');
    documents.dispose();
  });
});
