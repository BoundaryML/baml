type EventCallback<TPartial> = (err: unknown, event: FakeEvent<TPartial>) => void

class FakeEvent<TPartial> {
  constructor(private readonly value: TPartial) {}

  isOk(): boolean {
    return true
  }

  parsed(_isStreaming?: boolean): TPartial {
    return this.value
  }
}

class FakeFinal<TFinal> {
  constructor(private readonly value: TFinal) {}

  parsed(_isStreaming?: boolean): TFinal {
    return this.value
  }
}

class FakeRuntimeStream<TPartial, TFinal> {
  private callback?: EventCallback<TPartial>
  private dispatchPromises: Promise<void>[] = []

  constructor(
    private readonly partials: TPartial[],
    private readonly finalValue: TFinal,
    private readonly delayMs: number,
  ) {}

  onEvent(callback?: EventCallback<TPartial>): void {
    this.callback = callback

    if (!callback) {
      this.dispatchPromises = []
      return
    }

    this.dispatchPromises = this.partials.map((partial, index) => {
      return new Promise<void>(resolve => {
        const timeout = this.delayMs * index
        setTimeout(() => {
          this.callback?.(null, new FakeEvent(partial))
          resolve()
        }, timeout)
      })
    })
  }

  async done(): Promise<FakeFinal<TFinal>> {
    await Promise.all(this.dispatchPromises)
    return new FakeFinal(this.finalValue)
  }
}

export function createFakeRuntimeStream<TPartial, TFinal>(
  partials: TPartial[],
  finalValue: TFinal,
  delayMs = 0,
) {
  return new FakeRuntimeStream(partials, finalValue, delayMs)
}
