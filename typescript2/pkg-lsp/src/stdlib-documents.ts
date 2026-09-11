import type * as VSCode from 'vscode';

export const STDLIB_SCHEME = 'baml-stdlib';
export const STDLIB_SOURCE_METHOD = 'baml/stdlibSource';

export interface StdlibSourceResult {
  content: string;
}

type LoadSource = (
  uri: string,
  token: VSCode.CancellationToken,
) => Promise<StdlibSourceResult>;

export class StdlibDocuments implements VSCode.Disposable {
  private load: LoadSource | undefined;
  private readonly changed: VSCode.EventEmitter<VSCode.Uri>;
  private readonly registration: VSCode.Disposable;

  constructor(
    private readonly vscode: Pick<typeof VSCode, 'workspace' | 'EventEmitter'>,
  ) {
    this.changed = new vscode.EventEmitter<VSCode.Uri>();
    this.registration = vscode.workspace.registerTextDocumentContentProvider(
      STDLIB_SCHEME,
      {
        onDidChange: this.changed.event,
        provideTextDocumentContent: async (uri, token) => {
          const load = this.load;
          if (!load)
            throw new Error(
              'The BAML language server for this stdlib document is unavailable.',
            );
          return (await load(uri.toString(), token)).content;
        },
      },
    );
  }

  connect(load: LoadSource): VSCode.Disposable {
    this.load = load;
    for (const document of this.vscode.workspace.textDocuments) {
      if (document.uri.scheme === STDLIB_SCHEME)
        this.changed.fire(document.uri);
    }
    return {
      dispose: () => {
        if (this.load === load) this.load = undefined;
      },
    };
  }

  dispose(): void {
    this.registration.dispose();
    this.changed.dispose();
    this.load = undefined;
  }
}
