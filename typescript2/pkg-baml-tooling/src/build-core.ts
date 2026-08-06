import type { LoadProjectOptions } from './node.js';
import type { BamlProject } from './project.js';

export interface BamlPluginOptions {
  root?: string;
  include?: RegExp;
  exclude?: RegExp;
  target?: 'node' | 'web';
  cacheDir?: string | false;
  debug?: boolean;
}

export interface BuildHost {
  addWatchFile?(path: string): void;
  invalidate?(id: string): void;
  fullReload?(): void;
  development?: boolean;
}

export type ProjectFactory = (
  options: LoadProjectOptions,
) => Promise<BamlProject>;

export class BamlBuildCore {
  constructor(
    options: BamlPluginOptions = {},
    factory?: ProjectFactory,
  ) {
    void options;
    void factory;
    throw new Error('not implemented');
  }

  async start(host: BuildHost = {}): Promise<void> {
    void host;
    throw new Error('not implemented');
  }

  enableDevelopment(): void {
    throw new Error('not implemented');
  }

  async resolve(id: string, importer?: string): Promise<string | undefined> {
    void id;
    void importer;
    throw new Error('not implemented');
  }

  load(id: string): { code: string; watchFiles: string[] } | undefined {
    void id;
    throw new Error('not implemented');
  }

  watchChange(
    path: string,
    text: string | null | undefined,
    host: BuildHost = {},
  ): void {
    void path;
    void text;
    void host;
    throw new Error('not implemented');
  }

  close(): void {
    throw new Error('not implemented');
  }
}
