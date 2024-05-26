import { BamlSpanPy, RuntimeContextManagerPy, BamlRuntimePy } from './native'
import { AsyncLocalStorage } from 'async_hooks'

export class CtxManager {
  private rt: BamlRuntimePy
  private ctx: AsyncLocalStorage<RuntimeContextManagerPy>

  constructor(rt: BamlRuntimePy) {
    this.rt = rt
    this.ctx = new AsyncLocalStorage<RuntimeContextManagerPy>()
    this.ctx.enterWith(rt.createContextManager())
    process.on('exit', () => {
      this.rt.flush()
    })
  }

  upsertTags(tags: Record<string, string>): void {
    const manager = this.ctx.getStore()!
    manager.upsertTags(tags)
  }

  get(): RuntimeContextManagerPy {
    let store = this.ctx.getStore()
    if (store === undefined) {
      store = this.rt.createContextManager()
      this.ctx.enterWith(store)
    }
    return store
  }

  startTraceSync(name: string, args: Record<string, any>): BamlSpanPy {
    const mng = this.get()
    return BamlSpanPy.new(this.rt, name, args, mng)
  }

  startTraceAsync(name: string, args: Record<string, any>): BamlSpanPy {
    const mng = this.get()
    const clone = mng.deepClone()
    this.ctx.enterWith(clone)
    return BamlSpanPy.new(this.rt, name, args, clone)
  }

  async endTrace(span: BamlSpanPy, response: any): Promise<void> {
    await span.finish(response, this.get())
  }

  traceFnSync<ReturnType, F extends (...args: any[]) => Promise<ReturnType>>(name: string, func: F): F {
    return <F>((...args: any[]) => {
      const params = args.reduce(
        (acc, arg, i) => ({
          ...acc,
          [func.length > i ? func.arguments[i].name : `<arg:${i}>`]: arg,
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

  traceFnAync<ReturnType, F extends (...args: any[]) => Promise<ReturnType>>(func: F): F {
    const funcName = func.name
    return <F>(async (...args: any[]) => {
      const params = args.reduce(
        (acc, arg, i) => ({
          ...acc,
          [func.length > i ? func.arguments[i].name : `<arg:${i}>`]: arg,
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
