import assert from 'assert'
import { Image } from '@boundaryml/baml'
import { b, NamedArgsSingleEnumList, flush } from '../baml_client'

describe('Integ tests', () => {
  it('should work for all inputs', async () => {
    let res = await b.TestFnNamedArgsSingleBool(true)
    expect(res).toEqual('true')

    res = await b.TestFnNamedArgsSingleStringList(['a', 'b', 'c'])
    expect(res).toContain('a')
    expect(res).toContain('b')
    expect(res).toContain('c')

    console.log('calling with class')
    res = await b.TestFnNamedArgsSingleClass({
      key: 'key',
      key_two: true,
      key_three: 52,
    })
    console.log('got response', res)
    expect(res).toContain('52')

    res = await b.TestMulticlassNamedArgs(
      {
        key: 'key',
        key_two: true,
        key_three: 52,
      },
      {
        key: 'key',
        key_two: true,
        key_three: 64,
      },
    )
    expect(res).toContain('52')
    expect(res).toContain('64')

    res = await b.TestFnNamedArgsSingleEnumList([NamedArgsSingleEnumList.TWO])
    expect(res).toContain('TWO')

    res = await b.TestFnNamedArgsSingleFloat(3.12)
    expect(res).toContain('3.12')

    res = await b.TestFnNamedArgsSingleInt(3566)
    expect(res).toContain('3566')

    // TODO fix the fact it's required.
    res = await b.FnNamedArgsSingleStringOptional()
  })

  it('should work for all outputs', async () => {
    const a = 'a' // dummy
    let res = await b.FnOutputBool(a)
    expect(res).toEqual(true)

    const list = await b.FnOutputClassList(a)
    expect(list.length).toBeGreaterThan(0)
    assert(list[0].prop1.length > 0)

    const classWEnum = await b.FnOutputClassWithEnum(a)
    expect(['ONE', 'TWO']).toContain(classWEnum.prop2)

    const classs = await b.FnOutputClass(a)
    expect(classs.prop1).not.toBeNull()
    // Actually select 540
    expect(classs.prop2).toEqual(540)

    // enum list output
    const enumList = await b.FnEnumListOutput(a)
    expect(enumList.length).toBe(2)

    const myEnum = await b.FnEnumOutput(a)
  })

  it('works with retries1', async () => {
    try {
      await b.TestRetryConstant()
      assert(false)
    } catch (e) {
      console.log('Expected error', e)
    }
  })

  it('works with retries2', async () => {
    try {
      await b.TestRetryExponential()
      assert(false)
    } catch (e) {
      console.log('Expected error', e)
    }
  })

  it('works with fallbacks', async () => {
    const res = await b.TestFallbackClient()
    expect(res.length).toBeGreaterThan(0)
  })

  it('should work with image', async () => {
    let res = await b.TestImageInput(
      Image.fromUrl('https://upload.wikimedia.org/wikipedia/en/4/4d/Shrek_%28character%29.png'),
    )
    expect(res.toLowerCase()).toContain('green')
  })

  it('should support streaming in OpenAI', async () => {
    const stream = b.stream.PromptTestOpenAI('Mt Rainier is tall')
    const msgs: string[] = []
    for await (const msg of stream) {
      msgs.push(msg ?? '')
    }
    const final = await stream.getFinalResponse()

    expect(final.length).toBeGreaterThan(0)
    expect(msgs.length).toBeGreaterThan(0)
    for (let i = 0; i < msgs.length - 2; i++) {
      expect(msgs[i + 1].startsWith(msgs[i])).toBeTruthy()
    }
    expect(msgs.at(-1)).toEqual(final)
  })

  it('should support streaming without iterating', async () => {
    const final = await b.stream.PromptTestOpenAI('Mt Rainier is tall').getFinalResponse()
    expect(final.length).toBeGreaterThan(0)
  })

  it('should support streaming in Claude', async () => {
    const stream = b.stream.PromptTestClaude('Mt Rainier is tall')
    const msgs: string[] = []
    for await (const msg of stream) {
      msgs.push(msg ?? '')
    }
    const final = await stream.getFinalResponse()

    expect(final.length).toBeGreaterThan(0)
    expect(msgs.length).toBeGreaterThan(0)
    for (let i = 0; i < msgs.length - 2; i++) {
      expect(msgs[i + 1].startsWith(msgs[i])).toBeTruthy()
    }
    expect(msgs.at(-1)).toEqual(final)
  })
})

afterAll(async () => {
  flush()
})
