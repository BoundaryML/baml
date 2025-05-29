import { NamedArgsSingleEnumList } from '../baml_client'
import { partial_types } from '../baml_client/partial_types'
import { b, b_sync } from './test-setup'

describe('Basic Input/Output Tests', () => {
  describe('Input Types', () => {
    it('single bool', async () => {
      const res = await b.TestFnNamedArgsSingleBool(true)
      expect(res).toEqual('true')
    })

    it('single string list', async () => {
      const res = await b.TestFnNamedArgsSingleStringList(['a', 'b', 'c'])
      expect(res).toContain('a')
      expect(res).toContain('b')
      expect(res).toContain('c')
    })

    it('single class', async () => {
      const res = await b.TestFnNamedArgsSingleClass({
        key: 'key',
        key_two: true,
        key_three: 52,
      })
      expect(res).toContain('52')
    })

    it('multiple classes', async () => {
      const res = await b.TestMulticlassNamedArgs(
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
    })

    it('single enum list', async () => {
      const res = await b.TestFnNamedArgsSingleEnumList([NamedArgsSingleEnumList.TWO])
      expect(res).toContain('TWO')
    })

    it('single float', async () => {
      const res = await b.TestFnNamedArgsSingleFloat(3.12)
      expect(res).toContain('3.12')
    })

    it('single int', async () => {
      const res = await b.TestFnNamedArgsSingleInt(3566)
      expect(res).toContain('3566')
    })
  })

  describe('Output Types', () => {
    it('single bool', async () => {
      const input = 'test input'
      const bool = await b.FnOutputBool(input)
      expect(bool).toEqual(true)
    })

    it('single int', async () => {
      const input = 'test input'
      const int = await b.FnOutputInt(input)
      expect(int).toEqual(5)
    })

    it('single class', async () => {
      const input = 'test input'
      const classs = await b.FnOutputClass(input)
      expect(classs.prop1).not.toBeNull()
      expect(classs.prop2).toEqual(540)
    })

    it('single class list', async () => {
      const input = 'test input'
      const list = await b.FnOutputClassList(input)
      expect(list.length).toBeGreaterThan(0)
      expect(list[0].prop1.length).toBeGreaterThan(0)
    })

    it('enum list', async () => {
      const enumList = await b.FnEnumListOutput('input')
      expect(enumList.length).toBe(2)
    })

    it('single class with enum', async () => {
      const input = 'test input'
      const classWEnum = await b.FnOutputClassWithEnum(input)
      expect(['ONE', 'TWO']).toContain(classWEnum.prop2)
    })

    it('optional list and map', async () => {
      let res = await b.AllowedOptionals({ p: null, q: null })
      expect(res).toEqual({ p: null, q: null })

      res = await b.AllowedOptionals({ p: ['test'], q: { test: 'ok' } })
      expect(res).toEqual({ p: ['test'], q: { test: 'ok' } })
    })

    it('single optional string', async () => {
      const res = await b.FnNamedArgsSingleStringOptional()
      expect(res).toContain('none')
    })

    it('single map string to string', async () => {
      const res = await b.TestFnNamedArgsSingleMapStringToString({ lorem: 'ipsum', dolor: 'sit' })
      expect(res).toHaveProperty('lorem', 'ipsum')
    })

    it('single map string to map', async () => {
      const res = await b.TestFnNamedArgsSingleMapStringToMap({ lorem: { word: 'ipsum' }, dolor: { word: 'sit' } })
      expect(res).toHaveProperty('lorem', { word: 'ipsum' })
    })

    it('literal int', async () => {
      const int = await b.FnOutputLiteralInt('input')
      expect(int).toEqual(5)
    })

    it('literal bool', async () => {
      const bool = await b.FnOutputLiteralBool('input')
      expect(bool).toEqual(false)
    })

    it('literal string', async () => {
      const str = await b.FnOutputLiteralString('input')
      expect(str).toEqual('example output')
    })
  })
})

describe('Clients Tests', () => {
  it('should work with a sync client', () => {
    const res = b_sync.TestFnNamedArgsSingleStringList(['a', 'b', 'c'])
    expect(res).toContain('a')
  })
})

describe('Streaming Tests', () => {
  it('should support streaming without iterating', async () => {
    const final = await b.stream.PromptTestStreaming('Mt Rainier is tall').getFinalResponse()
    expect(final.length).toBeGreaterThan(0)
  })

  it('should work with nested classes', async () => {
    let stream = b.stream.FnOutputClassNested('hi!')
    let msgs: partial_types.TestClassNested[] = []
    for await (const msg of stream) {
      console.log('msg', msg)
      msgs.push(msg)
    }

    const final = await stream.getFinalResponse()
    expect(msgs.length).toBeGreaterThan(0)
    expect(msgs.at(-1)).toEqual(final)
  })
})

describe('Semantic Streaming Tests', () => {
  it('should support semantic streaming', async () => {
    const stream = b.stream.MakeSemanticContainer()

    let reference_string = null
    let reference_int = null

    const msgs: partial_types.SemanticContainer[] = []
    for await (const msg of stream) {
      msgs.push(msg ?? '')

      // Test field stability.
      if (msg.sixteen_digit_number != null) {
        if (reference_int == null) {
          reference_int = msg.sixteen_digit_number
        } else {
          expect(msg.sixteen_digit_number).toEqual(reference_int)
        }
      }

      // Test @stream.with_state.
      if (
        msg.class_needed.s_20_words?.value &&
        msg.class_needed.s_20_words.value.split(' ').length < 3 &&
        msg.final_string == null
      ) {
        expect(msg.class_needed.s_20_words.state).toEqual('Incomplete')
      }
      if (msg.final_string) {
        expect(msg.class_needed.s_20_words?.state).toEqual('Complete')
      }

      // Test @stream.not_null.
      if (msg.three_small_things) {
        for (const sub of msg.three_small_things) {
          expect(sub?.i_16_digits).toBeDefined()
        }
      }
    }

    const final = await stream.getFinalResponse()
  }, 20_000)
})
