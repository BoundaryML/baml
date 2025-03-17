import { b, b_sync } from './test-setup';


describe('Expose Parser Tests', () => {
  it('should expose parser', () => {
    const llmResponse = `
      \`\`\`json
      {
          "len": 5,
          "head": {
              "data": 1,
              "next": {
                  "data": 2,
                  "next": {
                      "data": 3,
                      "next": {
                          "data": 4,
                          "next": {
                              "data": 5,
                              "next": null
                          }
                      }
                  }
              }
          }
      }
      \`\`\`
    `;

    expect(b.parse.BuildLinkedList(llmResponse)).toEqual({
      head: {
        data: 1,
        next: {
          data: 2,
          next: {
            data: 3,
            next: {
              data: 4,
              next: {
                data: 5,
                next: null,
              },
            },
          },
        },
      },
      len: 5,
    })
  })

  it('should expose parser sync', () => {
    const llmResponse = `
      \`\`\`json
      {
          "len": 5,
          "head": {
              "data": 1,
              "next": {
                  "data": 2,
                  "next": {
                      "data": 3,
                      "next": {
                          "data": 4,
                          "next": {
                              "data": 5,
                              "next": null
                          }
                      }
                  }
              }
          }
      }
      \`\`\`
    `;

    expect(b_sync.parse.BuildLinkedList(llmResponse)).toEqual({
      head: {
        data: 1,
        next: {
          data: 2,
          next: {
            data: 3,
            next: {
              data: 4,
              next: {
                data: 5,
                next: null,
              },
            },
          },
        },
      },
      len: 5,
    })
  })

  it('should expose stream parser', () => {
    const stream = `
      \`\`\`json
      {
        "name": "John Doe",
        "email": "john.doe@example.com",
    `;

    expect(b.parseStream.ExtractResume(stream)).toEqual({
      name: "John Doe",
      email: "john.doe@example.com",
      phone: null,
      experience: [],
      education: [],
      skills: [],
    })
  })

  it('should expose stream parser sync', () => {
    const stream = `
      \`\`\`json
      {
        "name": "John Doe",
        "email": "john.doe@example.com",
    `;

    expect(b_sync.parseStream.ExtractResume(stream)).toEqual({
      name: "John Doe",
      email: "john.doe@example.com",
      phone: null,
      experience: [],
      education: [],
      skills: [],
    })
  })
})
