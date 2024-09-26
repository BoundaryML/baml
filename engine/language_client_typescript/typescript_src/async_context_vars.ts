import { BamlSpan, RuntimeContextManager, BamlRuntime, BamlLogEvent } from './native'
import { AsyncLocalStorage } from 'async_hooks'

export class BamlCtxManager {
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

  allowResets(): boolean {
    let store = this.ctx.getStore()
    if (store === undefined) {
      return true
    }
    if (store.contextDepth() > 0) {
      return false
    }

    return true
  }

  reset(): void {
    this.ctx = new AsyncLocalStorage<RuntimeContextManager>()
    this.ctx.enterWith(this.rt.createContextManager())
  }

  upsertTags(tags: Record<string, string>): void {
    const manager = this.ctx.getStore()!
    manager.upsertTags(tags)
  }

  cloneContext(): RuntimeContextManager {
    let store = this.ctx.getStore()
    if (store === undefined) {
      store = this.rt.createContextManager()
      this.ctx.enterWith(store)
    }
    return store.deepClone()
  }

  startTrace(name: string, args: Record<string, any>): [RuntimeContextManager, BamlSpan] {
    const mng = this.cloneContext()
    return [mng, BamlSpan.new(this.rt, name, args, mng)]
  }

  endTrace(span: BamlSpan, response: any): void {
    const manager = this.ctx.getStore()
    if (!manager) {
      console.error('Context lost before span could be finished\n')
      return
    }
    try {
      span.finish(response, manager)
    } catch (e) {
      console.error('BAML internal error', e)
    }
  }

  flush(): void {
    this.rt.flush()
  }

  onLogEvent(callback: ((event: BamlLogEvent) => void) | undefined): void {
    if (!callback) {
      this.rt.setLogEventCallback(undefined)
      return
    }
    this.rt.setLogEventCallback((error: any, param: BamlLogEvent) => {
      if (!error) {
        callback(param)
      }
    })
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
      const [mng, span] = this.startTrace(name, params)
      this.ctx.run(mng, () => {
        try {
          const response = func(...args)
          this.endTrace(span, response)
          return response
        } catch (e) {
          this.endTrace(span, e)
          throw e
        }
      })
    })
  }

  traceFnAsync<ReturnType, F extends (...args: any[]) => Promise<ReturnType>>(name: string, func: F): F {
    const funcName = name
    return <F>(async (...args: any[]) => {
      const params = args.reduce(
        (acc, arg, i) => ({
          ...acc,
          [`arg${i}`]: arg, // generic way to label args
        }),
        {},
      )
      const [mng, span] = this.startTrace(name, params)
      await this.ctx.run(mng, async () => {
        try {
          const response = await func(...args)
          this.endTrace(span, response)
          return response
        } catch (e) {
          this.endTrace(span, e)
          throw e
        }
      })
    })
  }
}
