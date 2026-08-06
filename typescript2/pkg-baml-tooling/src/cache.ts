export class DiskArtifactCache {
  constructor(readonly directory: string) {
    throw new Error('not implemented');
  }

  key(parts: readonly (string | Uint8Array)[]): string {
    void parts;
    throw new Error('not implemented');
  }

  async get<T>(key: string): Promise<T | undefined> {
    void key;
    throw new Error('not implemented');
  }

  async put<T>(key: string, value: T): Promise<void> {
    void key;
    void value;
    throw new Error('not implemented');
  }

  singleFlight<T>(key: string, produce: () => Promise<T>): Promise<T> {
    void key;
    void produce;
    throw new Error('not implemented');
  }

  path(key: string): string {
    void key;
    throw new Error('not implemented');
  }
}
