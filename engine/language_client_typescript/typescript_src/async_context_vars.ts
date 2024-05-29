import { BamlSpan, RuntimeContextManager, BamlRuntime } from './native'
import { AsyncLocalStorage } from 'async_hooks'

export class CtxManager {
  private rt: BamlRuntime
  private ctx: AsyncLocalStorage<RuntimeContextManager>

  constructor(rt: BamlRuntime) {
    this.rt = rt
    this.ctx = new AsyncLocalStorage<RuntimeContextManager>()
    this.ctx.enterWith(rt.createContextManager())
    process.on('exit', () => {
      this.rt.flush()
    })
  }

  upsertTags(tags: Record<string, string>): void {
    const manager = this.ctx.getStore()!
    manager.upsertTags(tags)
  }

  get(): RuntimeContextManager {
    let store = this.ctx.getStore()
    if (store === undefined) {
      store = this.rt.createContextManager()
      this.ctx.enterWith(store)
    }
    return store
  }

  startTraceSync(name: string, args: Record<string, any>): BamlSpan {
    const mng = this.get()
    return BamlSpan.new(this.rt, name, args, mng)
  }

  startTraceAsync(name: string, args: Record<string, any>): BamlSpan {
    const mng = this.get()
    const clone = mng.deepClone()
    this.ctx.enterWith(clone)
    return BamlSpan.new(this.rt, name, args, clone)
  }

  async endTrace(span: BamlSpan, response: any): Promise<void> {
    await span.finish(response, this.get())
  }

  flush(): void {
    this.rt.flush()
  }

  traceFnSync<ReturnType, F extends (...args: any[]) => ReturnType>(name: string, func: F): F {
    return <F>((...args: any[]) => {
      const params = args.reduce(
        (acc, arg, i) => ({
          ...acc,
          [`arg${i}`]: arg, // generic way to label args
        }),
        {},
      )
      const span = this.startTraceSync(name, params)
      try {
        const response = func(...args)
        this.endTrace(span, response)
        return response
      } catch (e) {
        this.endTrace(span, e)
        throw e
      }
    })
  }

  traceFnAync<ReturnType, F extends (...args: any[]) => Promise<ReturnType>>(name: string, func: F): F {
    const funcName = name
    return <F>(async (...args: any[]) => {
      const params = args.reduce(
        (acc, arg, i) => ({
          ...acc,
          [`arg${i}`]: arg, // generic way to label args
        }),
        {},
      )
      const span = this.startTraceAsync(funcName, params)
      try {
        const response = await func(...args)
        await this.endTrace(span, response)
        return response
      } catch (e) {
        await this.endTrace(span, e)
        throw e
      }
    })
  }
}
