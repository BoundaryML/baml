import { NamedArgsSingleEnumList } from '../baml_client'
import { SemanticContainer } from '../baml_client/partial_types';
import { b } from './test-setup'

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
    it('should work for all outputs', async () => {
      const input = 'test input'

      const bool = await b.FnOutputBool(input)
      expect(bool).toEqual(true)

      const int = await b.FnOutputInt(input)
      expect(int).toEqual(5)

      const list = await b.FnOutputClassList(input)
      expect(list.length).toBeGreaterThan(0)
      expect(list[0].prop1.length).toBeGreaterThan(0)

      const classWEnum = await b.FnOutputClassWithEnum(input)
      expect(['ONE', 'TWO']).toContain(classWEnum.prop2)

      const classs = await b.FnOutputClass(input)
      expect(classs.prop1).not.toBeNull()
      expect(classs.prop2).toEqual(540)
    })
  })

  // TODO: @antonio Move this to its own file/block or whatever.
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
    const res = await b.TakeRecAliasDep({value: data})
    expect(res.value).toEqual(data)
    expect(res.value.json.object.list).toEqual([1, 2, 3])
  })
})

describe('Semantic Streaming Tests', () => {
  it('should support semantic streaming', async () => {
    const stream = b.stream.MakeSemanticContainer()

    let reference_string = null;
    let reference_int = null;

    const msgs: SemanticContainer[] = []
    for await (const msg of stream) {
      msgs.push(msg ?? '')

      // Test field stability.
      if (msg.sixteen_digit_number != null){
        if (reference_int == null) {
          reference_int = msg.sixteen_digit_number;
        } else {
          expect(msg.sixteen_digit_number).toEqual(reference_int);
        }
      }

      // Test @stream.with_state.
      if (msg.class_needed.s_20_words.value && msg.class_needed.s_20_words.value.split(" ").length < 3 && msg.final_string == null) {
        expect(msg.class_needed.s_20_words.state).toEqual("Incomplete");
      }
      if (msg.final_string) {
        expect(msg.class_needed.s_20_words.state).toEqual("Complete");
      }

      // Test @stream.not_null.
      if (msg.three_small_things) {
        for (const sub of msg.three_small_things) {
          expect(sub.i_16_digits).toBeDefined();
        }
      }
    }

    const final = await stream.getFinalResponse();
  }, 20_000)
})
