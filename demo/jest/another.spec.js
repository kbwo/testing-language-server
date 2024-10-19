describe("another", () => {
  it("fail", () => {
    expect(1).toBe(0);
  });

  it("pass", () => {
    expect(1).toBe(1);
  });
});

test("toplevel test", () => {
  expect(1).toBe(2);
});
