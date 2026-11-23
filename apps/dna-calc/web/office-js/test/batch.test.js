const assert = require("assert");
const { Excel } = require("../office.js");
const { test } = require("node:test");

test("proxies queue a single batch and hydrate range values", async () => {
  const sent = [];
  const transport = async (json) => {
    const message = JSON.parse(json);
    sent.push(message.request);
    const req = message.request.AutomationBatch;
    if (!req) return JSON.stringify({ Reply: { id: message.id, reply: "AutomationReleased" } });
    const results = [];
    req.ops.forEach((op, ordinal) => {
      if (op.op === "load") results.push({ ordinal, loaded: [["values", { t: "array", rows: 1, cols: 2, v: [{ t: "r8", bits: "8000000000000000" }, { t: "empty" }] }], ["address", { t: "text", v: "Sheet1!A1:B1" }]] });
      if (op.op === "call" && op.name === "getItemOrNullObject") results.push({ ordinal, null_object: true });
    });
    return JSON.stringify({ Reply: { id: message.id, reply: { AutomationBatch: { batch_id: req.batch_id, results } } } });
  };
  let r;
  await Excel.run(async (ctx) => {
    const sheet = ctx.workbook.worksheets.getItem("Sheet1");
    r = sheet.getRange("A1:B1"); r.load("values, address");
    assert.throws(() => r.values, (e) => e.code === "PropertyNotLoaded");
    const nul = ctx.workbook.worksheets.getItemOrNullObject("Nope");
    assert.strictEqual(sent.length, 0, "no host call before sync");
    r.values = [[1, 2]];
    await ctx.sync();
    assert.strictEqual(sent.length, 1);
    assert.ok(Object.is(r.values[0][0], -0), "-0 survives");
    assert.strictEqual(r.values[0][1], "");
    assert.strictEqual(r.address, "Sheet1!A1:B1");
    assert.strictEqual(nul.isNullObject, true);
  }, transport);
  assert.strictEqual(sent[0].AutomationBatch.ops[0].op, "root");
  assert.deepStrictEqual(sent[1].AutomationRelease, { context: sent[0].AutomationBatch.context });
});

test("worker envelopes carry one sync, void calls, and explicit release", async () => {
  const sent = [];
  const transport = async (json) => {
    const message = JSON.parse(json);
    sent.push(message);
    const batch = message.request.AutomationBatch;
    const reply = batch ? {
      AutomationBatch: { batch_id: batch.batch_id, results: batch.ops.map((_, ordinal) => ({ ordinal })) },
    } : "AutomationReleased";
    return JSON.stringify({ Reply: { id: message.id, reply } });
  };
  await Excel.run(async (ctx) => {
    ctx.workbook.worksheets.getItem("Sheet1").getRange("A1").clear();
    await ctx.sync();
  }, transport);
  assert.strictEqual(sent.length, 2);
  assert.strictEqual(sent[0].request.AutomationBatch.ops.at(-1).out, undefined);
  assert.deepStrictEqual(sent[1].request.AutomationRelease, { context: sent[0].request.AutomationBatch.context });
});

test("a rejected Excel.run callback discards unsubmitted operations", async () => {
  const sent = [];
  const transport = async (json) => {
    const message = JSON.parse(json);
    sent.push(message.request);
    const batch = message.request.AutomationBatch;
    const reply = batch
      ? { AutomationBatch: { batch_id: batch.batch_id, results: [] } }
      : "AutomationReleased";
    return JSON.stringify({ Reply: { id: message.id, reply } });
  };
  let context;
  await assert.rejects(Excel.run(async (ctx) => {
    context = ctx;
    ctx.workbook.worksheets.getItem("Sheet1").getRange("A1").values = [[4]];
    throw new Error("callback failed");
  }, transport), /callback failed/);
  await context.sync();
  assert.deepStrictEqual(sent, [{ AutomationRelease: { context: context._id } }]);
});
