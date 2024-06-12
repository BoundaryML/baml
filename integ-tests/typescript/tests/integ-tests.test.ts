import assert from 'assert'
import { Image } from '@boundaryml/baml'
import { b, NamedArgsSingleEnumList, flush, traceAsync, traceSync, setTags } from '../baml_client'
import TypeBuilder from "../baml_client/type_builder";

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

  it('should support streaming in Gemini', async () => {
    const stream = b.stream.TestGemini('Dr. Pepper')
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

  

  it('supports tracing sync', async () => {
    const blah = 'blah'

    const res = traceSync('myFuncParent', (firstArg: string, secondArg: number) => {
      setTags({ myKey: 'myVal' })

      console.log('hello world')

      const res2 = traceSync('dummyFunc', dummyFunc)('dummyFunc')
      console.log('dummyFunc returned')

      const res3 = traceSync('dummyFunc2', dummyFunc)(firstArg)
      console.log('dummyFunc2 returned')

      return 'hello world'
    })('myFuncParent', 10)

    // adding this console log makes it work.
    // console.log('res returned', res)

    traceSync('dummyFunc3', dummyFunc)('hi there')
  })

  // Look at the dashboard to verify results.
  it('supports tracing async', async () => {
    const res = await traceAsync('parentAsync', async (firstArg: string, secondArg: number) => {
      console.log('hello world')
      setTags({ myKey: 'myVal' })

      const res1 = traceSync('dummyFunc', dummyFunc)('firstDummyFuncArg')

      const res2 = await traceAsync('asyncDummyFunc', asyncDummyFunc)('secondDummyFuncArg')

      const llm_res = await b.TestFnNamedArgsSingleStringList(['a', 'b', 'c'])

      const res3 = await traceAsync('asyncDummyFunc', asyncDummyFunc)('thirdDummyFuncArg')

      return 'hello world'
    })('hi', 10)

    const res2 = await traceAsync('parentAsync2', async (firstArg: string, secondArg: number) => {
      console.log('hello world')

      const res1 = traceSync('dummyFunc', dummyFunc)('firstDummyFuncArg')

      return 'hello world'
    })('hi', 10)
  })

  it('should work with dynamics', async () => {
    let tb = new TypeBuilder();
    tb.Person.addProperty("last_name", tb.string().optional());
    tb.Person.addProperty("height", tb.float().optional()).description("Height in meters");
    tb.Hobby.addValue("CHESS")
    tb.Hobby.listValues().map(([name, v]) => v.alias(name.toLowerCase()))
    tb.Person.addProperty("hobbies", tb.Hobby.type().list().optional()).description("Some suggested hobbies they might be good at");

    const res = await b.ExtractPeople("My name is Harrison. My hair is black and I'm 6 feet tall. I'm pretty good around the hoop.", { tb })
    expect(res.length).toBeGreaterThan(0)
    console.log(res)
  })
})

function asyncDummyFunc(myArg: string): Promise<MyInterface> {
  console.log('asyncDummyFuncArgs', arguments)
  return new Promise((resolve) => {
    resolve({
      key: 'key',
      key_two: true,
      key_three: 52,
    })
  })
}

interface MyInterface {
  key: string
  key_two: boolean
  key_three: number
}

function dummyFunc(myArg: string): string {
  return 'hello world'
}

afterAll(async () => {
  flush()
})
