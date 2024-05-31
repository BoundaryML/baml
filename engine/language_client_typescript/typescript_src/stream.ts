import { FunctionResult, FunctionResultStream, RuntimeContextManager } from './native'

export class BamlStream<PartialOutputType, FinalOutputType> {
  private task: Promise<FunctionResult> | null = null

  private eventQueue: (FunctionResult | null)[] = []

  constructor(
    private ffiStream: FunctionResultStream,
    private partialCoerce: (result: FunctionResult) => PartialOutputType,
    private finalCoerce: (result: FunctionResult) => FinalOutputType,
    private ctxManager: RuntimeContextManager,
  ) {}

  private async driveToCompletion(): Promise<FunctionResult> {
    try {
      this.ffiStream.onEvent((err, data) => {
        if (err) {
          return
        } else {
          this.eventQueue.push(data)
        }
      })
      const retval = await this.ffiStream.done(this.ctxManager)

      return retval
    } finally {
      this.eventQueue.push(null)
    }
  }

  private driveToCompletionInBg(): Promise<FunctionResult> {
    if (this.task === null) {
      this.task = this.driveToCompletion()
    }

    return this.task
  }

  async *[Symbol.asyncIterator](): AsyncIterableIterator<PartialOutputType> {
    this.driveToCompletionInBg()

    while (true) {
      const event = this.eventQueue.shift()

      if (event === undefined) {
        await new Promise((resolve) => setTimeout(resolve, 100))
        continue
      }

      if (event === null) {
        break
      }

      yield this.partialCoerce(event.parsed())
    }
  }

  async getFinalResponse(): Promise<FinalOutputType> {
    const final = await this.driveToCompletionInBg()

    return this.finalCoerce(final.parsed())
  }
}
