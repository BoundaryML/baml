/**
 * AsyncIterableQueue - Convert callback-based APIs to async generators
 *
 * Allows buffering events from callbacks and yielding them asynchronously.
 */

export class AsyncIterableQueue<T> {
  private queue: T[] = [];
  private resolvers: Array<(value: IteratorResult<T>) => void> = [];
  private done = false;
  private error: Error | null = null;

  /**
   * Push an item to the queue
   * If consumers are waiting, resolve them immediately
   */
  push(item: T): void {
    if (this.done) {
      throw new Error('Cannot push to a completed queue');
    }

    if (this.resolvers.length > 0) {
      // Consumer is waiting, resolve immediately
      const resolve = this.resolvers.shift()!;
      resolve({ value: item, done: false });
    } else {
      // Buffer the item
      this.queue.push(item);
    }
  }

  /**
   * Signal that no more items will be pushed
   */
  complete(): void {
    this.done = true;
    // Resolve all waiting consumers with done=true
    while (this.resolvers.length > 0) {
      const resolve = this.resolvers.shift()!;
      resolve({ value: undefined as any, done: true });
    }
  }

  /**
   * Signal an error occurred
   */
  fail(err: Error): void {
    this.error = err;
    this.done = true;
    // Reject all waiting consumers
    while (this.resolvers.length > 0) {
      const resolve = this.resolvers.shift()!;
      resolve({ value: undefined as any, done: true });
    }
  }

  /**
   * Async iterator implementation
   */
  async *[Symbol.asyncIterator](): AsyncIterator<T> {
    while (true) {
      // Check for error
      if (this.error) {
        throw this.error;
      }

      // If we have buffered items, yield them
      if (this.queue.length > 0) {
        yield this.queue.shift()!;
        continue;
      }

      // If done and queue is empty, we're finished
      if (this.done) {
        return;
      }

      // Wait for next item
      const result = await new Promise<IteratorResult<T>>((resolve) => {
        this.resolvers.push(resolve);
      });

      if (result.done) {
        return;
      }

      yield result.value;
    }
  }
}
