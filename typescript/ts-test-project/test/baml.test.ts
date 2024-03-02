
describe("describe", () => {
  test("test1", () => {
    expect(1).toBe(1);

    console.log(expect.getState().currentTestName);
  });
});

test("root_test2", async () => {
  // sleep for 4 secs
  await new Promise((resolve) => setTimeout(resolve, 3000));
  expect(1).toBe(1);
  expect(2).toBe(1);
});
