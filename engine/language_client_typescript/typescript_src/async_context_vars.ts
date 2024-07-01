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

  upsertTags(tags: Record<string, string>): void {
    const manager = this.ctx.getStore()!
    manager.upsertTags(tags)
  }

  get(): RuntimeContextManager {
    console.log('get current RuntimeContextManager')
    let store = this.ctx.getStore()
    if (store === undefined) {
      console.error('no store found in current AsyncLocalContext')
      store = this.rt.createContextManager()
      this.ctx.enterWith(store)
    }
    return store
  }

  startTraceSync(name: string, args: Record<string, any>): [RuntimeContextManager, BamlSpan] {
    const mng = this.get()
    return [mng, BamlSpan.new(this.rt, name, args, mng)]
  }

  startTraceAsync(name: string, args: Record<string, any>): [RuntimeContextManager, BamlSpan] {
    const mng = this.get().deepClone()
    return [mng, BamlSpan.new(this.rt, name, args, mng)]
  }

  endTrace(span: BamlSpan, response: any): void {
    const manager = this.ctx.getStore()
    if (!manager) {
      console.error('Context lost before span could be finished\n')
      return
    }
    span.finish(response, manager)
  }

  flush(): void {
    this.rt.flush()
  }

  onLogEvent(callback: (event: BamlLogEvent) => void): void {
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
      const [mng, span] = this.startTraceSync(name, params)
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
      const [mng, span] = this.startTraceAsync(name, params)
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
