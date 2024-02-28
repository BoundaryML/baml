import { trace, initTracer, setTags, traceAsync } from "baml-client-lib";


initTracer();

const fx = trace((a: number, b: number) => {
  setTags({ "foo": 'bar' })
  return a + b;
}, 'add', [{ name: 'a', type: 'number' }, { name: 'b', type: 'number' }], false, 'number');

const fx2 = trace((a: number, b: number) => {
  setTags({ "bar": null })
  return a - b;
}, 'sub', [{ name: 'a', type: 'number' }, { name: 'b', type: 'number' }], false, 'number');


const combo = trace((a: number, b: number) => {
  setTags({ "bar": "Test" })
  return fx(a, b) + fx2(a, b);
}, 'combo', [{ name: 'a', type: 'number' }, { name: 'b', type: 'number' }], false, 'number');

const fx2Async = traceAsync((a: number, b: number) => {
  // setTags({ "foo": 'bar' })
  // Simulate a delay
  return new Promise<number>(resolve => setTimeout(() => resolve(a - b), 1000));
}, 'childFunc', [{ name: 'a', type: 'number' }, { name: 'b', type: 'number' }], false, 'number');

const fxAsync = traceAsync(async (a: number, b: number) => {
  // setTags({ "foo": 'bar' })
  // Simulate a delay
  await new Promise(resolve => setTimeout(resolve, 1000));
  return a + b + (await fx2Async(a, b));
}, 'parentFunc', [{ name: 'a', type: 'number' }, { name: 'b', type: 'number' }], false, 'number');

// const res = fx(1, 2);
// const res2 = fx2(1, 2);
// const res3 = combo(1, 2);

// console.log({
//   // res,
//   // res2,
//   res3
// });


fxAsync(1, 2).then(res => {
  console.log(res);
});

// BAML.scope(async () => {
//   BAML.scope(async () => {
//     await fxAsync(1, 2)
//   })
// })
/**
 * helpful links
 * https://github.com/getsentry/sentry-javascript/blob/afcec2d33a243f9d6ec869c69a96c54ce4c3f88d/packages/opentelemetry/src/asyncContextStrategy.ts#L51
 *
 *
 * const result2 = await Sentry.startSpan(
  { name: "Important Function" },
  async () => {
    const res = await Sentry.startSpan({ name: "Child Span" }, () => {
      return expensiveAsyncFunction();
    });

    return updateRes(res);
  }
);

const result3 = Sentry.startSpan({ name: "Important Function" }, (span) => {
  // You can access the span to add attributes or set specific status.
  // The span may be undefined if the span was not sampled or if performance monitoring is disabled.
  span?.setAttribute("foo", "bar");
  return expensiveFunction();
});

 *
 */
// import { AsyncLocalStorage } from 'async_hooks';

// class Data {
//   content: Array<[span]>;

//   constructor() {
//     this.content = new Array();
//   }

//   copy() {
//     const data = new Data();
//     data.content = this.content.slice();
//     return data;
//   }

//   set(value: string): string {
//     const id = Math.random().toString(36).substring(7);
//     this.content.push([id, value]);
//     // console log the current span
//     console.log(this.content.map(([id, value]) => value).join(' -> '));
//     return id;
//   }

//   pop(id: string) {
//     let last = this.content.pop();
//     if (last && last[0] !== id) {
//       throw new Error('Invalid id');
//     }
//   }
// }

// const asyncLocalStorage = new AsyncLocalStorage<Data>();

// const scope = async <T>(name: string, cb: () => Promise<T>): Promise<T> => {
//   // start span: https://github.com/getsentry/sentry-javascript/blob/afcec2d33a243f9d6ec869c69a96c54ce4c3f88d/packages/opentelemetry/src/trace.ts#L22 
//   // calls this:
//   // https://github.com/getsentry/sentry-javascript/blob/afcec2d33a243f9d6ec869c69a96c54ce4c3f88d/packages/node/src/async/hooks.ts#L34
//   // Get the current local storage or create a new one if it doesn't exist
//   const store = asyncLocalStorage.getStore();

//   const handle = async (data: Data) => {
//     const id = data.set(name);
//     try {
//       return await cb();
//     } finally {
//       data.pop(id);
//     }
//   }

//   // set the new span attri
//   if (!store) {
//     const storage = new Data();
//     // update storage here with parent info
//     console.log('----new span----');
//     const res = await asyncLocalStorage.run(storage, () => handle(asyncLocalStorage.getStore()!));
//     return res;
//   } else {
//     const res = await asyncLocalStorage.run(store.copy(), () => handle(asyncLocalStorage.getStore()!));
//     return res;
//   }
// }

// const fn = async () => {
//   const res = await scope('fn', async () => {
//     return 'fn';
//   });
//   return res;
// }

// const fn2 = async () => {
//   const res = await scope('fn2', async () => {
//     return 'fn2';
//   });
//   return res;
// }

// const fn3 = async () => {
//   const res = await scope('fn3', async () => {
//     await Promise.all([fn(), fn2()]);
//     return 'fn3';
//   });
//   return res;
// }

// const fn4 = async () => {
//   const res = await scope('fn3', async () => {
//     await Promise.all([fn(), fn2()]);
//     return 'fn3';
//   });
//   return res;
// }

// (async () => {
//   await fn3();
//   // await fn4();
// })().then(() => {
//   console.log('done');
// });

// // shutdownTracer();


// // let fnx = (async (a: number, b: number) => {
// //   await new Promise(resolve => setTimeout(resolve, 1000));
// //   return a + b;
// // });

// // console.log(fnx);

// // fnx(1, 2).then(res => {
// //   console.log(res);
// // });