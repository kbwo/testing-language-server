import { assert, assertEquals } from "jsr:@std/assert";
import { add } from "./main.ts";

const throwFn = () => {
  throw new Error("error");
};

Deno.test(function addTest() {
  assertEquals(add(2, 3), 5);
});

Deno.test(function fail1() {
  assertEquals(add(2, 5), 5);
});

Deno.test(function fail2() {
  assert(throwFn());
});
