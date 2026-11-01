// Run against the bundled product worker: the shared seven-cell trace and one long-cell
// continuation both hand off to a real worker task. The seven-cell inputs have one source.
return (async function () {
  const fixture = $FIXTURE;
  const pureScope = $PURE_SCOPE;
  const decode_tile_event = window.__dnacalcDecodeTile;
  const worker = new Worker(new URL("worker.js", window.__dnacalcAssets), { type: "module" });
  const pending = new Map(), trace = [], cells = new Map();
  let requestId = 0, commandId = 0, summary, selection = null, measuring = false;
  let started, completed, rejected, readyRejected, staged;
  const ready = new Promise((resolve, reject) => { started = resolve; readyRejected = reject; });
  const done = new Promise((resolve, reject) => { completed = resolve; rejected = reject; });
  done.catch(() => {});
  function require(condition, message) { if (!condition) throw new Error(message); }
  function abort(error) {
    for (const waiting of pending.values()) waiting.reject(error);
    pending.clear(); readyRejected(error); rejected(error);
  }
  function stageTile(batch) {
    require(staged && JSON.stringify(staged.stamp) === JSON.stringify(batch.stamp), "tile fragment changed transaction identity");
    require(batch.fragment === staged.fragment++, "tile fragments were not ordered");
    staged.cells.push(...batch.cells); staged.removed.push(...batch.removed);
    staged.closed = batch.final_fragment;
  }
  worker.onmessage = function (event) {
    try {
      const tile = decode_tile_event(event.data);
      if (tile != null) { stageTile(JSON.parse(tile)); return; }
      const message = JSON.parse(Array.isArray(event.data) ? event.data[0] : event.data);
      if (message === "Started") { started(); return; }
      if (message.Event) {
        const control = message.Event;
        if (control.Calculation) {
          const receipt = control.Calculation.receipt;
          trace.push(receipt);
          if (measuring && receipt.Completed) {
            require(control.Calculation.failed_projections.length === 0, "calculation projection failed");
            completed(receipt.Completed);
          }
        } else if (control.TileAdmission) {
          require(!staged, "overlapping projection transactions");
          staged = { stamp: control.TileAdmission, fragment: 0, closed: false, cells: [], removed: [] };
        } else if (control.TileCompletion) {
          const barrier = control.TileCompletion;
          require(staged && staged.closed && staged.stamp.id === barrier.id && staged.stamp.next === barrier.through,
            "projection completion arrived before all fragments");
          const delta = { publication: staged.stamp.calculation_through, reset: staged.stamp.reset,
            cells: staged.cells, emptied: staged.removed };
          if (delta.reset) cells.clear();
          for (const key of delta.emptied) cells.delete(JSON.stringify(key));
          for (const cell of delta.cells) cells.set(JSON.stringify(cell.at), cell);
          trace.push({ Cells: delta }); staged = null;
          send({ TileAck: barrier }).catch(abort);
        } else if (control.ProjectionFailed) {
          throw new Error(control.ProjectionFailed.reason);
        } else if (control.Tiles) {
          throw new Error("worker continuation bypassed the binary tile codec");
        }
        return;
      }
      const response = message.Reply || message.Failed;
      if (!response) return;
      if (message.Reply && response.reply.Pong) trace.push({ Pong: true });
      const waiting = pending.get(response.id);
      if (waiting) {
        pending.delete(response.id);
        if (message.Failed) waiting.reject(new Error(response.message));
        else waiting.resolve(response.reply);
      } else if (message.Failed) { throw new Error(response.message); }
    } catch (error) { abort(error); }
  };
  worker.onerror = function (event) { abort(new Error(event.message)); };
  function send(request) {
    const id = ++requestId;
    return new Promise((resolve, reject) => {
      pending.set(id, { resolve, reject });
      worker.postMessage(JSON.stringify({ id, request }));
    });
  }
  async function intent(value) {
    const reply = await send({ Command: {
      session: summary.session, id: ++commandId, workbook: summary.workbook,
      base_revision: summary.revision, source: "engine-contract-smoke", undo_group: null,
      resolved_selection: selection, request: { Intent: value }
    } });
    require(reply.Done && reply.Done.outcome.accepted, "the worker refused the fixture command");
    summary = reply.Done.outcome.summary;
    selection = reply.Done.outcome.selection;
    return reply;
  }
  try {
    await ready;
    summary = (await send("Summary")).Summary;
    const sheet = summary.sheets[0][0];
    const key = (row, col) => [sheet[0], sheet[1], row, col];
    const area = (row, col, lastRow = row, lastCol = col) => [key(row, col), key(lastRow, lastCol)];
    await intent({ SetCalcMode: { manual: true } });
    for (const [row, col, text] of fixture) await intent({ EnterCell: { at: key(row, col), text } });
    await intent({ EnterCell: { at: key(1, 1), text: "2" } });
    await intent({ EnterCell: { at: key(1, 6), text: "10" } });
    // Ordinary independent cells make several real worker tasks necessary at the product's
    // finite budget. They do not add a second seven-cell fixture or alter its formulas.
    const background = area(1, 9, 4096, 9);
    await intent({ Select: { active: key(1, 9), areas: [background] } });
    await intent({ EnterSelection: { text: "=ROW()" } });
    await send({ Subscribe: { id: 1, interest: { Areas: [area(1, 2, 1, 5), area(1, 5, 10, 5), area(10, 7)] } } });
    trace.length = 0;
    measuring = true;
    const scope = pureScope.map(([row, col]) => area(row, col)).concat([background]);
    // Queue Ping before the first continuation timer can be admitted by the worker.
    const calculation = intent({ Calculate: { scope: { Range: scope }, selection: "DirtyAndVolatile" } });
    const ping = send("Ping");
    await calculation;
    require((await ping).Pong, "the real worker did not answer Ping");
    // Supersede unfinished dirty work through an actual authored command. E1 has already
    // produced its independent candidate; the dirty B1/C1 cone must be evaluated again.
    await intent({ EnterCell: { at: key(1, 1), text: "20" } });
    const receipt = await done;
    require(receipt.outcome === "Complete" && receipt.calculated === 4100 && receipt.refused === 0,
      "the pure scoped calculation did not complete all admitted targets");
    require(trace.find((event) => event.Admitted)?.Admitted.selection === "DirtyAndVolatile",
      "the reuse demonstration must preserve its declared dirty selection");
    const yielded = trace.findIndex((event) => event.Yielded);
    const pong = trace.findIndex((event) => event.Pong);
    const complete = trace.findIndex((event) => event.Completed);
    require(yielded >= 0 && yielded < pong && pong < complete, "Ping was not serviced between calculation slices");
    const superseded = trace.findIndex((event) => event.Superseded);
    require(pong < superseded && superseded < complete, "the authored edit did not supersede unfinished work");
    const predecessor = trace[superseded].Superseded.command;
    const successor = trace[superseded].Superseded.by;
    require(trace.some((event) => event.Reused && JSON.stringify(event.Reused.at) === JSON.stringify(key(1, 5))
      && event.Reused.computed_by === predecessor && event.Reused.certified_for === successor),
      "the independent E1 candidate was not reused for the successor");
    require(trace.some((event) => event.Rejected && JSON.stringify(event.Rejected.at) === JSON.stringify(key(1, 2))),
      "the changed B1 candidate was not rejected");
    require(!trace.some((event) => event.Completed?.command === predecessor), "the superseded command completed");
    const publications = trace.filter((event) => event.Cells && event.Cells.publication);
    require(publications.length === 1, "the spill must install with one atomic publication");
    const output = (row, col) => cells.get(JSON.stringify(key(row, col)));
    for (const [row, col, text] of [[1, 2, "21"], [1, 3, "42"], [10, 7, "11"]]) {
      require(output(row, col)?.text === text, "a pure seven-cell result differs from the hand-derived value");
    }
    for (let row = 1; row <= 10; row++) {
      require(output(row, 5)?.text === String(row), "the complete SEQUENCE spill was not installed");
    }
    require(output(1, 4)?.provenance === "Stale", "the P01/P03 callback was evaluated outside the pure scope");
    const spillScope = [area(1, 5), area(10, 7)];
    await intent({ EnterCell: { at: key(1, 6), text: "1" } });
    await intent({ CalculateRange: { areas: spillScope } });
    require(output(1, 5)?.text === "1" && output(10, 7)?.text === "1", "shrinking the spill did not repair G10");
    for (let row = 2; row <= 10; row++) require(!output(row, 5), "a removed spill cell remains published");
    await intent({ EnterCell: { at: key(1, 6), text: "10" } });
    await intent({ EnterCell: { at: key(10, 5), text: "99" } });
    await intent({ CalculateRange: { areas: spillScope } });
    require(output(1, 5)?.text === "#SPILL!", "authored E10 did not block the spill");
    require(output(10, 5)?.text === "99" && output(10, 5)?.provenance === "Literal", "the spill overwrote authored E10");
    require(output(10, 7)?.text === "100", "G10 did not read the authored blocker");
    const replayStart = trace.length;
    // C1 alone would cross the P02 B1-cache boundary. Repeating the explicit pure scope
    // proves equal-value status delivery without making that unimplemented policy implicit.
    await intent({ CalculateRange: { areas: pureScope.map(([row, col]) => area(row, col)) } });
    const replay = trace.slice(replayStart);
    const replayAdmission = replay.findIndex((event) => event.Admitted);
    const replayPublication = replay.findIndex((event) => event.Published);
    const replayCompletion = replay.findIndex((event) => event.Completed);
    require(replayAdmission >= 0 && replayAdmission < replayPublication && replayPublication < replayCompletion,
      "an equal-value calculation did not deliver ordered status receipts");
    const replayCommand = replay[replayAdmission].Admitted.command;
    require(replayCommand !== successor && replay[replayCompletion].Completed.command === replayCommand
      && replay[replayCompletion].Completed.calculated === 4
      && replay[replayCompletion].Completed.outcome === "Complete" && output(1, 3)?.text === "42",
      "an equal-value replay must certify the same C1 value for a new command");
    const equalText = output(1, 3)?.text;
    // One admitted formula must yield inside its SEQUENCE kernel; there are no background
    // targets in this command from which an inter-cell handoff could be inferred.
    await intent({ EnterCell: { at: key(1, 10), text: "=SUM(SEQUENCE(262144))+1" } });
    await send({ SetInterest: { id: 1, interest: { Areas: [area(1, 10)] } } });
    const longStart = trace.length;
    const longDone = new Promise((resolve, reject) => { completed = resolve; rejected = reject; });
    longDone.catch(() => {});
    const longCalculation = intent({ Calculate: { scope: { Range: [area(1, 10)] }, selection: "ForceAll" } });
    const longPing = send("Ping");
    await longCalculation;
    require((await longPing).Pong, "the worker did not answer Ping during one long formula");
    const longReceipt = await longDone;
    const longTrace = trace.slice(longStart);
    const longYield = longTrace.findIndex((event) => event.Yielded);
    const longPong = longTrace.findIndex((event) => event.Pong);
    const longComplete = longTrace.findIndex((event) => event.Completed);
    require(longYield >= 0 && longYield < longPong && longPong < longComplete,
      "the single formula did not hand off before completion");
    require(longReceipt.calculated === 1 && longReceipt.refused === 0 && longReceipt.outcome === "Complete",
      "the long-cell command must have exactly one admitted calculated target");
    require(output(1, 10)?.text === "34359869441", "the resumed long-cell result differs from n*(n+1)/2+1");
    return { yielded, pong, complete, calculated: receipt.calculated, publications: publications.length,
      reused_spill: { from: predecessor, to: successor }, spill_companions: ["shrink", "authored-blocker"],
      equal_value_status: { command: replayCommand, text: equalText },
      in_cell: { targets: longReceipt.calculated, yielded: longYield, pong: longPong, completed: longComplete, text: output(1, 10)?.text } };
  } finally {
    worker.terminate();
  }
})();
