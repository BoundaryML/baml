import { FunctionResultPy, FunctionResultStreamPy, RuntimeContextManagerPy } from '../native'

export class BamlStream<PartialOutputType, FinalOutputType> {
  private task: Promise<FunctionResultPy> | null = null

  private eventQueue: (FunctionResultPy | null)[] = []

  constructor(
    private ffiStream: FunctionResultStreamPy,
    private partialCoerce: (result: FunctionResultPy) => PartialOutputType,
    private finalCoerce: (result: FunctionResultPy) => FinalOutputType,
    private ctxManager: RuntimeContextManagerPy,
  ) {}

  private async driveToCompletion(): Promise<FunctionResultPy> {
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

  private driveToCompletionInBg(): Promise<FunctionResultPy> {
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

  async done(): Promise<FinalOutputType> {
    const final = await this.driveToCompletionInBg()

    return this.finalCoerce(final.parsed())
  }
}
