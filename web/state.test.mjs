import test from "node:test";
import assert from "node:assert/strict";
import { initialState, reduce } from "./state.js";

test("all three intentions stay in one state machine", () => {
  let s = initialState();
  for (const value of ["act", "understand", "discover"]) {
    s = reduce(s, { type: "select-intention", value });
    assert.equal(s.intention, value);
  }
  assert.equal(s.revision, 4);
});

test("WHAT IF remains explicitly hypothetical", () => {
  let s = initialState();
  s = reduce(s, { type: "add-premise", text: "Roberto never saw the email", hypothetical: true });
  assert.equal(s.premises[0].kind, "hypothetical");
  s = reduce(s, { type: "set-focus", value: "What changes?" });
  assert.equal(s.premises[0].kind, "hypothetical");
});

test("retraction creates a revision without deleting premise history", () => {
  let s = initialState();
  s = reduce(s, { type: "add-premise", text: "Alice attended", hypothetical: false });
  const id = s.premises[0].id;
  s = reduce(s, { type: "retract-premise", id });
  assert.equal(s.premises.length, 1);
  assert.equal(s.premises[0].active, false);
  assert.match(s.history.at(-1).reason, /retracted/);
});

test("source catalogue never labels a file analyzed", () => {
  let s = initialState();
  s = reduce(s, { type: "add-source", name: "chat.txt" });
  assert.deepEqual(s.sources, ["chat.txt"]);
  assert.equal(s.history.at(-1).reason, "Source catalogued: chat.txt");
});
