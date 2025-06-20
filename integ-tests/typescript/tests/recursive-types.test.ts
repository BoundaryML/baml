import { b } from "./test-setup";

describe("Recursive Type Tests", () => {
  it("simple recursive type", async () => {
    const res = await b.BuildLinkedList([1, 2, 3, 4, 5]);
    expect(res).toEqual({
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
    });
  });

  it("mutually recursive type", async () => {
    const res = await b.BuildTree({
      data: 5,
      left: {
        data: 3,
        left: {
          data: 1,
          left: null,
          right: {
            data: 2,
            left: null,
            right: null,
          },
        },
        right: {
          data: 4,
          left: null,
          right: null,
        },
      },
      right: {
        data: 7,
        left: {
          data: 6,
          left: null,
          right: null,
        },
        right: {
          data: 8,
          left: null,
          right: null,
        },
      },
    });
    expect(res).toEqual({
      data: 5,
      children: {
        trees: [
          {
            data: 3,
            children: {
              trees: [
                {
                  data: 1,
                  children: {
                    trees: [
                      {
                        data: 2,
                        children: {
                          trees: [],
                        },
                      },
                    ],
                  },
                },
                {
                  data: 4,
                  children: {
                    trees: [],
                  },
                },
              ],
            },
          },
          {
            data: 7,
            children: {
              trees: [
                {
                  data: 6,
                  children: {
                    trees: [],
                  },
                },
                {
                  data: 8,
                  children: {
                    trees: [],
                  },
                },
              ],
            },
          },
        ],
      },
    });
  });
});
