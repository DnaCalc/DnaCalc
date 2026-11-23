const assert = require("assert");
const { Excel } = require("../office.js");
(async () => {
  const sent = [];
  const transport = async (json) => {
    const req = JSON.parse(json); sent.push(req);
    const results = [];
    req.ops.forEach((op, ordinal) => {
      if (op.op === "load") results.push({ ordinal, loaded: [["values", { t: "array", rows: 1, cols: 2, v: [{ t: "r8", bits: "8000000000000000" }, { t: "empty" }] }], ["address", { t: "text", v: "Sheet1!A1:B1" }]] });
      if (op.op === "call" && op.name === "getItemOrNullObject") results.push({ ordinal, null_object: true });
    });
    return JSON.stringify({ batch_id: req.batch_id, results });
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
  // Excel.run's final sync is a second batch only when ops remain or release is needed.
  assert.strictEqual(sent.length, 2); assert.strictEqual(sent[1].final_release, true);
  assert.strictEqual(sent[0].ops[0].op, "root");
  console.log("ok", sent[0].ops.map(o => o.op).join(","));
})().catch((e) => { console.error(e); process.exit(1); });
