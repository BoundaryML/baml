import { b } from './test-setup';


describe('Prompt Renderer Tests', () => {
  it('maintain field order', async () => {
    const request = await b.request.UseMaintainFieldOrder(
      {
        a: "1",
        b: "2",
        c: "3",
      }
    )

    expect(request.body.json()).toEqual({
      model: 'gpt-4o-mini',
      messages: [
        {
          role: 'system',
          content: [
            {
                type: 'text',
                text: `Return this value back to me: {
    "a": "1",
    "b": "2",
    "c": "3",
}`
            }
          ]
        }
      ],
    })
  })
})