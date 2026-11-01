// Run against the bundled product worker: the shared seven-cell trace and one long-cell
// continuation both hand off to a real worker task. The seven-cell inputs have one source.
return (async function () {
  const fixture = $FIXTURE;
  const pureScope = $PURE_SCOPE;
  const decode_tile_event = window.__dnacalcDecodeTile;
  const worker = new Worker(new URL("worker.js", window.__dnacalcAssets), { type: "module" });
  const pending = new Map(), trace = [], cells = new Map();
  let requestId = 0, commandId = 0, summary, selection = null, measuring = false;
  let started, completed, rejected, readyRejected, staged, watchPublication;
  const ready = new Promise((resolve, reject) => { started = resolve; readyRejected = reject; });
  const done = new Promise((resolve, reject) => { completed = resolve; rejected = reject; });
  done.catch(() => {});
  function require(condition, message) { if (!condition) throw new Error(message); }
  function abort(error) {
    for (const waiting of pending.values()) waiting.reject(error);
    pending.clear(); readyRejected(error); rejected(error);
    if (watchPublication) watchPublication(error);
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
          // A reset may replay unchanged cells beside a newer calculation frontier.
          // Compare complete cells against the prior committed root before replacing it.
          delta.changed = delta.cells.filter((cell) =>
            JSON.stringify(cells.get(JSON.stringify(cell.at))) !== JSON.stringify(cell));
          if (delta.reset) cells.clear();
          for (const key of delta.emptied) cells.delete(JSON.stringify(key));
          for (const cell of delta.cells) cells.set(JSON.stringify(cell.at), cell);
          trace.push({ Cells: delta }); staged = null;
          if (watchPublication) watchPublication();
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
        else {
          if (Array.isArray(event.data)) response.reply.package_bytes = event.data[1];
          waiting.resolve(response.reply);
        }
      } else if (message.Failed) { throw new Error(response.message); }
    } catch (error) { abort(error); }
  };
  worker.onerror = function (event) { abort(new Error(event.message)); };
  function send(request, bytes) {
    const id = ++requestId;
    return new Promise((resolve, reject) => {
      pending.set(id, { resolve, reject });
      const text = JSON.stringify({ id, request });
      if (bytes) worker.postMessage([text, bytes], [bytes]);
      else worker.postMessage(text);
    });
  }
  async function command(request, bytes) {
    const reply = await send({ Command: {
      session: summary.session, id: ++commandId, workbook: summary.workbook,
      base_revision: summary.revision, source: "engine-contract-smoke", undo_group: null,
      resolved_selection: selection, request
    } }, bytes);
    require(reply.Done && reply.Done.outcome.accepted, "the worker refused the fixture command");
    summary = reply.Done.outcome.summary;
    selection = reply.Done.outcome.selection;
    return reply;
  }
  function intent(value) { return command({ Intent: value }); }
  try {
    await ready;
    summary = (await send("Summary")).Summary;
    let sheet = summary.sheets[0][0];
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
    await send({ Subscribe: { id: 1, workbook: summary.workbook, interest: { Areas: [area(1, 2, 1, 5), area(1, 5, 10, 5), area(10, 7)] } } });
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
    // D1 is an excluded unresolved array producer. Keep conservative workbook-wide Stale
    // provenance while checking the one unit that installs every hand-derived numeric member.
    const spillPublications = publications.filter((event) => event.Cells.changed.some((cell) =>
      cell.at[3] === 5 && cell.at[2] >= 1 && cell.at[2] <= 10));
    require(spillPublications.length === 1, "the spill must install with one atomic publication: "
      + JSON.stringify(spillPublications.map(({ Cells: delta }) => ({ publication: delta.publication,
        reset: delta.reset, spill: delta.changed.filter((cell) => cell.at[3] === 5
          && cell.at[2] >= 1 && cell.at[2] <= 10).map((cell) => [cell.at[2], cell.text, cell.provenance]) }))));
    require(spillPublications[0].Cells.cells.filter((cell) => cell.at[3] === 5
      && cell.at[2] >= 1 && cell.at[2] <= 10).length === 10, "a spill publication omitted part of its extent");
    require(spillPublications[0].Cells.changed.filter((cell) => cell.at[3] === 5
      && cell.at[2] >= 1 && cell.at[2] <= 10).length === 10, "the spill changed only part of its extent");
    const output = (row, col) => cells.get(JSON.stringify(key(row, col)));
    for (const [row, col, text] of [[1, 2, "21"], [1, 3, "42"], [10, 7, "11"]]) {
      require(output(row, col)?.text === text, "a pure seven-cell result differs from the hand-derived value");
    }
    for (let row = 1; row <= 10; row++) {
      require(output(row, 5)?.text === String(row) && output(row, 5)?.provenance === "Stale", "the complete SEQUENCE spill was not installed");
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
    // Repeat the pure scope, then narrow to C1 through the measured P02 scalar-cache policy.
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
    const narrowStart = trace.length;
    const narrowDone = new Promise((resolve, reject) => { completed = resolve; rejected = reject; });
    narrowDone.catch(() => {});
    await intent({ CalculateRange: { areas: [area(1, 3)] } });
    const narrowReceipt = await narrowDone;
    const narrowTrace = trace.slice(narrowStart);
    const narrowAdmission = narrowTrace.findIndex((event) => event.Admitted);
    const narrowPublication = narrowTrace.findIndex((event) => event.Published);
    const narrowCompletion = narrowTrace.findIndex((event) => event.Completed);
    require(narrowAdmission >= 0 && narrowAdmission < narrowPublication && narrowPublication < narrowCompletion,
      "C1-only replay did not deliver ordered calculation receipts");
    const narrowAdmitted = narrowTrace[narrowAdmission].Admitted;
    require(JSON.stringify(narrowAdmitted.scope) === JSON.stringify({ Range: [area(1, 3)] })
      && narrowAdmitted.range_post_state === "StillRequired", "Manual C1-only admission changed its scope or post-state");
    require(narrowReceipt.calculated === 1 && narrowReceipt.refused === 0 && narrowReceipt.outcome === "Complete"
      && narrowReceipt.boundary_cached === 1 && narrowReceipt.imported_cached === 0,
      "C1-only replay did not retain its scalar boundary-cache qualification");
    require(output(1, 3)?.text === "42" && output(1, 2)?.text === "21", "C1-only replay changed the boundary result");
    const narrowText = output(1, 3)?.text;
    // One admitted formula must yield inside its SEQUENCE kernel; there are no background
    // targets in this command from which an inter-cell handoff could be inferred.
    await intent({ EnterCell: { at: key(1, 10), text: "=SUM(SEQUENCE(262144))+1" } });
    await send({ SetInterest: { id: 1, workbook: summary.workbook, interest: { Areas: [area(1, 10)] } } });
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
    const longText = output(1, 10)?.text;
    // A viewport promotion must publish the short prerequisite cone before the unrelated
    // long cell finishes, through the same binary projection and acknowledgement route.
    // The original sheet deliberately keeps D1 unresolved. Its possible spill makes
    // downstream cells conservatively stale; isolate this freshness proof on another sheet.
    const fixtureSheet = sheet;
    await intent({ EditSheet: { sheet: null, change: { Add: { name: "Priority" } } } });
    sheet = summary.sheets.find(([, name]) => name === "Priority")[0];
    for (const [col, text] of [[13, "=SUM(SEQUENCE(1048576))"], [14, "=3"], [15, "=N1*2"]]) {
      await intent({ EnterCell: { at: key(1, col), text } });
    }
    // Give the prerequisite its own unit at the actual multi-evaluation worker budget.
    await send({ SetInterest: { id: 1, workbook: summary.workbook, interest: { Areas: [area(1, 14)] } } });
    const priorityStart = trace.length;
    const priorityDone = new Promise((resolve, reject) => { completed = resolve; rejected = reject; });
    priorityDone.catch(() => {});
    const shortPublished = new Promise((resolve, reject) => {
      watchPublication = (error) => {
        if (error) { watchPublication = null; reject(error); return; }
        if (output(1, 15)?.text !== "6") return;
        require(output(1, 15)?.provenance === "Calculated",
          "the promoted cell's installed unit was not projected as calculated: " + JSON.stringify(output(1, 15)));
        const current = trace.slice(priorityStart);
        require(!current.some((event) => event.Completed), "the short cone appeared only at whole-command completion");
        const unit = current.filter((event) => event.Cells?.publication).at(-1)?.Cells.publication;
        require(unit && unit.sequence === 2 && unit.command === current.find((event) => event.Admitted)?.Admitted.command,
          "the short cone did not install after its prerequisite's unit");
        watchPublication = null;
        resolve({ command: unit.command, sequence: unit.sequence, text: output(1, 15)?.text, before_completion: true });
      };
    });
    shortPublished.catch(() => {});
    // The three fresh inputs are dirty; unlike Manual ForceAll, this demand is satisfied.
    const priorityCalculation = intent({ Calculate: { scope: { Range: [area(1, 13, 1, 15)] }, selection: "DirtyAndVolatile" } });
    const priorityPing = send("Ping");
    const promotion = send({ SetInterest: { id: 1, workbook: summary.workbook, interest: { Areas: [area(1, 15)] } } });
    await priorityCalculation;
    const priorityAdmission = trace.slice(priorityStart).find((event) => event.Admitted)?.Admitted;
    require(priorityAdmission?.selection === "DirtyAndVolatile" && priorityAdmission.range_post_state === "Satisfied",
      "the priority fixture did not admit dirty targets with a satisfied post-state");
    require((await priorityPing).Pong, "the priority command did not yield to Ping");
    await promotion;
    const priorityPublication = await shortPublished;
    const priorityReceipt = await priorityDone;
    require(priorityReceipt.calculated === 3 && priorityReceipt.refused === 0 && priorityReceipt.outcome === "Complete",
      "interest changed the priority command's exact three-target coverage");
    const priorityEvents = trace.slice(priorityStart);
    const publishedCone = priorityEvents.findIndex((event) => event.Published?.command === priorityPublication.command
      && event.Published.sequence === priorityPublication.sequence);
    require(publishedCone >= 0 && publishedCone < priorityEvents.findIndex((event) => event.Completed),
      "the worker exposed completion before the short cone's publication barrier");
    await intent({ EditSheet: { sheet: null, change: { Add: { name: "VBA Contract" } } } });
    sheet = summary.sheets.find(([, name]) => name === "VBA Contract")[0];
    const vbaSource = "Private entries As Long\nPublic Function CountAndPeek(ByVal r As DnaCalcObjects.Range, ByVal count As Long) As Double\n Dim total As Long\n Dim initial As Long\n initial=count\n entries=entries+1\n Do While count>0\n total=total+1\n count=count-1\n Loop\n CountAndPeek=r.NumericValue+total-initial\nEnd Function\nPublic Function EntryCount() As Long\n EntryCount=entries\nEnd Function\n";
    await command({ LoadVbaModule: { name: "Counted", source: vbaSource, max_steps: 5000000 } });
    for (const [col, text] of [[1, "1"], [2, "=A1+1"], [4, "=CountAndPeek(B1,200000)"], [5, "=EntryCount()"]]) {
      await intent({ EnterCell: { at: key(1, col), text } });
    }
    await send({ SetInterest: { id: 1, workbook: summary.workbook, interest: { Areas: [area(1, 1, 1, 5)] } } });
    await intent({ CalculateRange: { areas: [area(1, 2)] } });
    require(output(1, 2)?.text === "2", "the VBA cache was not calculated before the edit");
    await intent({ EnterCell: { at: key(1, 1), text: "10" } });
    const vbaStart = trace.length;
    const vbaDone = new Promise((resolve, reject) => { completed = resolve; rejected = reject; });
    vbaDone.catch(() => {});
    const vbaCalculation = intent({ CalculateRange: { areas: [area(1, 4)] } });
    const vbaPing = send("Ping");
    await vbaCalculation;
    require((await vbaPing).Pong, "the worker did not service Ping during the VBA activation");
    const vbaReceipt = await vbaDone;
    const vbaEvents = trace.slice(vbaStart);
    const vbaYield = vbaEvents.findIndex((event) => event.Yielded);
    const vbaPong = vbaEvents.findIndex((event) => event.Pong);
    const vbaComplete = vbaEvents.findIndex((event) => event.Completed);
    require(vbaYield >= 0 && vbaYield < vbaPong && vbaPong < vbaComplete,
      "the single-cell VBA procedure did not yield inside its interpreter activation");
    require(vbaReceipt.outcome === "Complete" && vbaReceipt.calculated === 1 && vbaReceipt.refused === 0,
      "the VBA command did not complete exactly its one admitted target");
    require(output(1, 4)?.text === "2" && output(1, 4)?.provenance === "Stale" && output(1, 2)?.text === "2",
      "the VBA callback did not retain its qualified old cache without demanding B1");
    await intent({ CalculateRange: { areas: [area(1, 5)] } });
    require(output(1, 5)?.text === "1", "resuming the VBA activation repeated its counter effect");
    const replacement = "Private entries As Long\nPublic Function CountAndPeek(ByVal value As Double, ByVal count As Long) As Double\n entries=entries+1\n CountAndPeek=value+50#\nEnd Function\nPublic Function EntryCount() As Long\n EntryCount=entries\nEnd Function\n";
    await command({ LoadVbaModule: { name: "Counted", source: replacement, max_steps: 100000 } });
    await intent({ CalculateRange: { areas: [area(1, 4)] } });
    require(output(1, 4)?.text === "52", "VBA reload did not bind the replacement signature and code");
    await intent({ CalculateRange: { areas: [area(1, 5)] } });
    require(output(1, 5)?.text === "1", "VBA reload did not start fresh project state");
    const vbaWitness = { targets: vbaReceipt.calculated, yielded: vbaYield, pong: vbaPong,
      completed: vbaComplete, old_cache: "2", entries: "1", reload: output(1, 4)?.text };
    const provisionalRanges = [];
    const rangeSource = $P03_VBA_SOURCE;
    for (const [formula, producer, seed, expected, stats, reevaluations, heldTarget] of $P03_RANGE_CASES) {
      const name = "P03 Range " + provisionalRanges.length;
      await intent({ EditSheet: { sheet: null, change: { Add: { name } } } });
      sheet = summary.sheets.find(([, title]) => title === name)[0];
      await command({ LoadVbaModule: { name: "ProvisionalRange", source: rangeSource, max_steps: 100000 } });
      if (heldTarget != null) {
        await intent({ EnterCell: { at: key(1, 8), text: "=HoldRange(INDIRECT(" + JSON.stringify(heldTarget) + "))" } });
        const heldDone = new Promise((resolve, reject) => { completed = resolve; rejected = reject; });
        heldDone.catch(() => {});
        await intent({ CalculateRange: { areas: [area(1, 8)] } });
        require((await heldDone).calculated === 1, "the hidden Range was not captured");
      }
      for (const [col, text] of [[2, formula], [3, "=B1+1"]]) {
        await intent({ EnterCell: { at: key(1, col), text } });
      }
      if (seed != null) {
        await intent({ EnterCell: { at: key(1, 6), text: "1" } });
        await intent({ EnterCell: { at: key(1, 4), text: "=" + seed } });
        const seedDone = new Promise((resolve, reject) => { completed = resolve; rejected = reject; });
        seedDone.catch(() => {});
        await intent({ CalculateRange: { areas: [area(1, 4)] } });
        require((await seedDone).calculated === 1, "the retained spill control was not calculated");
        await intent({ EnterCell: { at: key(1, 6), text: "20" } });
      } else { await intent({ EnterCell: { at: key(1, 4), text: "=" + producer } }); }
      await send({ SetInterest: { id: 1, workbook: summary.workbook,
        interest: { Areas: [area(1, 2, 1, 4), area(1, 26)] } } });
      const start = trace.length;
      const rangeDone = new Promise((resolve, reject) => { completed = resolve; rejected = reject; });
      rangeDone.catch(() => {});
      await intent({ CalculateRange: { areas: [area(1, 2, 1, 4)] } });
      const rangeReceipt = await rangeDone;
      require(rangeReceipt.outcome === "Complete" && rangeReceipt.calculated === 3 && rangeReceipt.refused === 0,
        "the provisional Range command lost its three-target coverage");
      require(output(1, 2)?.text === String(expected) && output(1, 3)?.text === String(expected + 1),
        "the Range read did not calculate its final value and consumer: " + formula);
      const repeated = trace.slice(start).filter((event) => event.Reevaluated).length;
      require(repeated === reevaluations, "Range semantic reevaluation count differs: " + formula);
      await intent({ EnterCell: { at: key(1, 26), text: "=EntryStats()" } });
      const statsDone = new Promise((resolve, reject) => { completed = resolve; rejected = reject; });
      statsDone.catch(() => {});
      await intent({ CalculateRange: { areas: [area(1, 26)] } });
      const statsReceipt = await statsDone;
      require(statsReceipt.outcome === "Complete" && statsReceipt.calculated === 1 && statsReceipt.refused === 0,
        "the Range counter command did not complete its one target");
      require(output(1, 26)?.text === String(stats), "Range entry/Empty-read counters differ: " + formula);
      provisionalRanges.push({ formula, value: String(expected), entry_stats: output(1, 26)?.text,
        reevaluations: repeated });
    }
    vbaWitness.provisional_ranges = provisionalRanges;
    sheet = fixtureSheet;
    // Exercise Save/Open over transferred package bytes, through the product worker itself.
    for (const [col, text] of [[17, "1"], [18, "=Q1*2"], [19, "=R1+1"]]) {
      await intent({ EnterCell: { at: key(1, col), text } });
    }
    const scalarDone = new Promise((resolve, reject) => { completed = resolve; rejected = reject; });
    scalarDone.catch(() => {});
    await intent({ CalculateRange: { areas: [area(1, 18, 1, 19)] } });
    require((await scalarDone).calculated === 2, "the save fixture did not calculate its scalar caches");
    await intent({ EnterCell: { at: key(1, 17), text: "5" } });
    const saved = await command("Save");
    require(Object.prototype.toString.call(saved.package_bytes) === "[object ArrayBuffer]" && saved.package_bytes.byteLength > 0,
      "Save did not transfer a package beside its reply");
    const packageBytes = saved.package_bytes.byteLength;
    const reopenStart = trace.length;
    const reopened = await command({ Open: { name: saved.Done.saved } }, saved.package_bytes);
    require(reopened.Done.opened?.[1] === null, "Manual reopen performed an unrequested calculation");
    const reopenedSheet = summary.sheets[0][0];
    const savedKey = (col) => [reopenedSheet[0], reopenedSheet[1], 1, col];
    await send({ Subscribe: { id: 1, workbook: summary.workbook, interest: { Areas: [[savedKey(17), savedKey(19)]] } } });
    const savedCell = (col) => cells.get(JSON.stringify(savedKey(col)));
    require(savedCell(17)?.text === "5" && savedCell(17)?.provenance === "Literal",
      "the saved input was replaced by a calculation cache");
    for (const [col, text] of [[18, "2"], [19, "3"]]) {
      require(savedCell(col)?.text === text && savedCell(col)?.provenance === "FileCache",
        "Manual Save/Open did not retain its unverified scalar cache");
    }
    require(!trace.slice(reopenStart).some((event) => event.Admitted), "Manual Open admitted hidden calculation work");
    const manualSave = { input: savedCell(17)?.text, caches: [savedCell(18)?.text, savedCell(19)?.text],
      provenance: "FileCache", bytes: packageBytes };
    return { yielded, pong, complete, calculated: receipt.calculated, publications: publications.length,
      manual_save: manualSave, vba: vbaWitness,
      spill_publications: spillPublications.length, interest_publication: priorityPublication,
      reused_spill: { from: predecessor, to: successor }, spill_companions: ["shrink", "authored-blocker"],
      equal_value_status: { command: replayCommand, text: equalText },
      narrow_cache_replay: { targets: narrowReceipt.calculated, boundary_cached: narrowReceipt.boundary_cached,
        post_state: narrowAdmitted.range_post_state, text: narrowText },
      in_cell: { targets: longReceipt.calculated, yielded: longYield, pong: longPong, completed: longComplete, text: longText } };
  } finally {
    worker.terminate();
  }
})();
