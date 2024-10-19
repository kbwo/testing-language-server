const test = require("node:test");
const { describe, it } = require("node:test");
const assert = require("node:assert");
const { throwError } = require("./util.js");
// # Basic example
test("synchronous passing test", (t) => {
  // This test passes because it does not throw an exception.
  assert.strictEqual(1, 1);
});

test("synchronous failing test", (t) => {
  // This test fails because it throws an exception.
  assert.strictEqual(1, 2);
});

test("asynchronous passing test", async (t) => {
  // This test passes because the Promise returned by the async
  // function is settled and not rejected.
  assert.strictEqual(1, 1);
});

test("asynchronous failing test", async (t) => {
  // This test fails because the Promise returned by the async
  // function is rejected.
  assert.strictEqual(1, 2);
});

test("failing test using Promises", (t) => {
  // Promises can be used directly as well.
  return new Promise((resolve, reject) => {
    setImmediate(() => {
      reject(new Error("this will cause the test to fail"));
    });
  });
});

test("callback passing test", (t, done) => {
  // done() is the callback function. When the setImmediate() runs, it invokes
  // done() with no arguments.
  setImmediate(done);
});

test("callback failing test", (t, done) => {
  // When the setImmediate() runs, done() is invoked with an Error object and
  // the test fails.
  setImmediate(() => {
    done(new Error("callback failure"));
  });
});

// # Subtests
test("top level test", async (t) => {
  await t.test("subtest 1", (t) => {
    assert.strictEqual(1, 1);
  });

  await t.test("subtest 2", (t) => {
    assert.strictEqual(2, 2);
  });
});

// # Skipping tests
// The skip option is used, but no message is provided.
test("skip option", { skip: true }, (t) => {
  // This code is never executed.
});

// The skip option is used, and a message is provided.
test("skip option with message", { skip: "this is skipped" }, (t) => {
  // This code is never executed.
});

test("skip() method", (t) => {
  // Make sure to return here as well if the test contains additional logic.
  t.skip();
});

test("skip() method with message", (t) => {
  // Make sure to return here as well if the test contains additional logic.
  t.skip("this is skipped");
});

// # TODO tests
// The todo option is used, but no message is provided.
test("todo option", { todo: true }, (t) => {
  // This code is executed, but not treated as a failure.
  throw new Error("this does not fail the test");
});

// The todo option is used, and a message is provided.
test("todo option with message", { todo: "this is a todo test" }, (t) => {
  // This code is executed.
});

test("todo() method", (t) => {
  t.todo();
});

test("todo() method with message", (t) => {
  t.todo("this is a todo test and is not treated as a failure");
  throw new Error("this does not fail the test");
});

// # describe() and it() aliases
describe("A thing", () => {
  it("should work", () => {
    assert.strictEqual(1, 1);
  });

  it("should be ok", () => {
    assert.strictEqual(2, 2);
  });

  describe("a nested thing", () => {
    it("should work", () => {
      assert.strictEqual(3, 3);
    });
  });
});

// # only tests
// Assume Node.js is run with the --test-only command-line option.
// The suite's 'only' option is set, so these tests are run.
test("only: this test is run", { only: true }, async (t) => {
  // Within this test, all subtests are run by default.
  await t.test("running subtest");

  // The test context can be updated to run subtests with the 'only' option.
  t.runOnly(true);
  await t.test("this subtest is now skipped");
  await t.test("this subtest is run", { only: true });

  // Switch the context back to execute all tests.
  t.runOnly(false);
  await t.test("this subtest is now run");

  // Explicitly do not run these tests.
  await t.test("skipped subtest 3", { only: false });
  await t.test("skipped subtest 4", { skip: true });
});

// The 'only' option is not set, so this test is skipped.
test("only: this test is not run", () => {
  // This code is not run.
  throw new Error("fail");
});

describe("A suite", () => {
  // The 'only' option is set, so this test is run.
  it("this test is run A ", { only: true }, () => {
    // This code is run.
  });

  it("this test is not run B", () => {
    // This code is not run.
    throw new Error("fail");
  });
});

describe.only("B suite", () => {
  // The 'only' option is set, so this test is run.
  it("this test is run C", () => {
    // This code is run.
  });

  it("this test is run D", () => {
    // This code is run.
  });
});

test("import from external file. this must be fail", () => {
  throwError();
});
