import { b } from './test-setup'

describe('Type aliases tests', () => {
  it('primitive union alias', async () => {
    const res = await b.PrimitiveAlias('test')
    expect(res).toEqual('test')
  })

  it('map alias', async () => {
    const res = await b.MapAlias({ A: ['B', 'C'], B: [], C: [] })
    expect(res).toEqual({ A: ['B', 'C'], B: [], C: [] })
  })

  it('alias union', async () => {
    let res = await b.NestedAlias('test')
    expect(res).toEqual('test')

    res = await b.NestedAlias({ A: ['B', 'C'], B: [], C: [] })
    expect(res).toEqual({ A: ['B', 'C'], B: [], C: [] })
  })

  it('alias pointing to recursive class', async () => {
    const res = await b.AliasThatPointsToRecursiveType({ value: 1, next: null })
    expect(res).toEqual({ value: 1, next: null })
  })

  it('class pointing to alias that points to recursive class', async () => {
    const res = await b.ClassThatPointsToRecursiveClassThroughAlias({ list: { value: 1, next: null } })
    expect(res).toEqual({ list: { value: 1, next: null } })
  })

  it('recursive class with alias indirection', async () => {
    const res = await b.RecursiveClassWithAliasIndirection({ value: 1, next: { value: 2, next: null } })
    expect(res).toEqual({ value: 1, next: { value: 2, next: null } })
  })

  it('merge alias attributes', async () => {
    const res = await b.MergeAliasAttributes(123)
    console.log(JSON.stringify(res));
    expect(res.amount.value).toEqual(123)
    expect(res.amount.checks['gt_ten'].status).toEqual('succeeded')
  })

  // Inputs with checks are not supported yet
  // it('return alias with merged attrs', async () => {
  //   const res = await b.ReturnAliasWithMergedAttributes({
  //     value: 123,
  //     checks: {
  //       gt_ten: {
  //         name: 'gt_ten',
  //         expr: 'value > 10',
  //         status: 'succeeded',
  //       },
  //     },
  //   })
  //   expect(res.value).toEqual(123)
  //   expect(res.checks['gt_ten'].status).toEqual('succeeded')
  // })

  // TODO: checks as inputs are not supported yet
  // it('alias with multiple attrs', async () => {
  //   const res = await b.AliasWithMultipleAttrs(123)
  //   expect(res.value).toEqual(123)
  //   expect(res.checks['gt_ten'].status).toEqual('succeeded')
  // })

  it('simple recursive map alias', async () => {
    const res = await b.SimpleRecursiveMapAlias({ one: { two: { three: {} } } })
    expect(res).toEqual({ one: { two: { three: {} } } })
  })

  it('simple recursive list alias', async () => {
    const res = await b.SimpleRecursiveListAlias([[], [], [[]]])
    expect(res).toEqual([[], [], [[]]])
  })

  it('recursive alias cycles', async () => {
    const res = await b.RecursiveAliasCycle([[], [], [[]]])
    expect(res).toEqual([[], [], [[]]])
  })

  it('json type alias cycle', async () => {
    const data = {
      number: 1,
      string: 'test',
      bool: true,
      list: [1, 2, 3],
      object: { number: 1, string: 'test', bool: true, list: [1, 2, 3] },
      json: {
        number: 1,
        string: 'test',
        bool: true,
        list: [1, 2, 3],
        object: { number: 1, string: 'test', bool: true, list: [1, 2, 3] },
      },
    }
    const res = await b.JsonTypeAliasCycle(data)
    expect(res).toEqual(data)
    expect(res.json.object.list).toEqual([1, 2, 3])
  })

  it('json type alias as class dependency', async () => {
    const data = {
      number: 1,
      string: 'test',
      bool: true,
      list: [1, 2, 3],
      object: { number: 1, string: 'test', bool: true, list: [1, 2, 3] },
      json: {
        number: 1,
        string: 'test',
        bool: true,
        list: [1, 2, 3],
        object: { number: 1, string: 'test', bool: true, list: [1, 2, 3] },
      },
    }
    const res = await b.TakeRecAliasDep({ value: data })
    expect(res.value).toEqual(data)
    expect(res.value.json?.object?.list).toEqual([1, 2, 3])
  })
})
