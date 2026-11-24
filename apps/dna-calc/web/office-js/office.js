/* DNA Calc clean-room Office.js-compatible client (MIT; dc-17spk.5). Written from the public API
   surface only; no Microsoft office.js code. Speaks the batch protocol of crates/calc-office-js:
   one message per context.sync(). Transport: globalThis.__dnaOfficeTransport(requestJson) -> Promise<replyJson>. */
(function (root) {
  "use strict";
  let nextMessage = 1, nextContext = 1;

  class OfficeError extends Error {
    constructor(code, message, extra) {
      super(message);
      this.name = "OfficeExtension.Error";
      this.code = code;
      this.debugInfo = Object.assign({ message }, extra || {});
    }
  }

  const f64 = new Float64Array(1), u64 = new BigUint64Array(f64.buffer);
  function r8Bits(v) { f64[0] = v; return u64[0].toString(16).padStart(16, "0"); }
  function r8From(bits) { u64[0] = BigInt("0x" + bits); return f64[0]; }
  function toWire(v, ctx) {
    if (v === undefined) return { t: "missing" };
    if (v === null) return { t: "null" };
    if (typeof v === "number") return { t: "r8", bits: r8Bits(v) };
    if (typeof v === "boolean") return { t: "bool", v };
    if (typeof v === "string") {
      if (/[\ud800-\udfff]/u.test(v)) {
        return { t: "text16", units: Array.from({ length: v.length }, (_, i) => v.charCodeAt(i)) };
      }
      return { t: "text", v };
    }
    if (v instanceof ClientObject) {
      if (v.context !== ctx) throw new OfficeError("InvalidObjectPath", "The object belongs to another request context.");
      return { t: "object", h: v._handle };
    }
    if (Array.isArray(v)) {
      const rows = v.length, cols = Array.isArray(v[0]) ? v[0].length : 0;
      if (Array.from(v).some(row => !Array.isArray(row) || row.length !== cols)) {
        throw new OfficeError("InvalidArgument", "Expected a rectangular two-dimensional array.");
      }
      return { t: "array", rows, cols, v: v.flatMap(row => Array.from(row, x => toWire(x, ctx))) };
    }
    throw new OfficeError("InvalidArgument", "unsupported argument type " + typeof v);
  }
  // Office.js presents Empty as "" and always 2-D for ranges; wire keeps the difference.
  function fromWire(w) {
    switch (w.t) {
      case "empty": case "null": case "missing": return "";
      case "bool": case "i4": return w.v;
      case "r8": return r8From(w.bits);
      case "text": return w.v;
      case "text16": return w.units.map(unit => String.fromCharCode(unit)).join("");
      case "error": return ERRORS[w.v] || "#VALUE!";
      case "array": {
        const out = [];
        for (let r = 0; r < w.rows; r++) out.push(w.v.slice(r * w.cols, (r + 1) * w.cols).map(fromWire));
        return out;
      }
      default: return undefined;
    }
  }
  const ERRORS = { 2000: "#NULL!", 2007: "#DIV/0!", 2015: "#VALUE!", 2023: "#REF!", 2029: "#NAME?", 2036: "#NUM!", 2042: "#N/A" };
  const TWO_D = new Set(["values", "formulas", "numberFormat", "text"]);

  class ClientObject {
    constructor(context, handle) {
      this.context = context; this._handle = handle; this._loaded = Object.create(null); this.isNullObject = false;
    }
    _step(name, args, Cls) {
      const out = this.context._alloc();
      this.context._push({ op: "call", target: this._handle, name, args: args.map((a) => toWire(a, this.context)), out });
      const o = new (Cls || ClientObject)(this.context, out);
      this.context._outs.set(out, o);
      return o;
    }
    _invoke(name) {
      this.context._push({ op: "call", target: this._handle, name });
    }
    load(props) {
      const list = (Array.isArray(props) ? props : String(props || "").split(",")).map((s) => s.trim()).filter(Boolean);
      const ordinal = this.context._push({ op: "load", target: this._handle, props: list });
      this.context._loads.set(ordinal, this);
      return this;
    }
    _prop(name) {
      if (!(name in this._loaded)) throw new OfficeError("PropertyNotLoaded",
        "The property '" + name + "' is not available. Before reading the property's value, call the load method on the containing object and call \"context.sync()\" on the associated request context.");
      return this._loaded[name];
    }
    _set(name, v) { this.context._push({ op: "set", target: this._handle, name, value: toWire(v, this.context) }); }
    toJSON() { return Object.assign({}, this._loaded); }
  }
  function props(Cls, names, writable) {
    for (const n of names) Object.defineProperty(Cls.prototype, n, {
      get() { return this._prop(n); },
      set: writable.includes(n) ? function (v) { this._set(n, v); } : undefined,
    });
  }

  class Range extends ClientObject { calculate() { this._invoke("calculate"); } clear() { this._invoke("clear"); } }
  props(Range, ["values", "formulas", "numberFormat", "text", "address", "rowIndex", "columnIndex"], ["values", "formulas", "numberFormat"]);
  class Worksheet extends ClientObject {
    getRange(a) { return this._step("getRange", a === undefined ? [] : [a], Range); }
    getUsedRangeOrNullObject() { return this._step("getUsedRangeOrNullObject", [], Range); }
    calculate() { this._invoke("calculate"); }
    activate() { this._invoke("activate"); }
  }
  props(Worksheet, ["name"], ["name"]);
  class WorksheetCollection extends ClientObject {
    getItem(k) { return this._step("getItem", [k], Worksheet); }
    getItemOrNullObject(k) { return this._step("getItemOrNullObject", [k], Worksheet); }
    add(n) { return this._step("add", n === undefined ? [] : [n], Worksheet); }
  }
  props(WorksheetCollection, ["count"], []);
  class Workbook extends ClientObject {
    get worksheets() { const out = this.context._alloc(); this.context._push({ op: "get", target: this._handle, name: "worksheets", out }); const o = new WorksheetCollection(this.context, out); this.context._outs.set(out, o); return o; }
  }
  class Application extends ClientObject { calculate() { this._invoke("calculate"); } }

  class TrackedObjects {
    constructor(ctx) { this.ctx = ctx; }
    add(o) { this._queue("track", o); }
    remove(o) { this._queue("untrack", o); }
    _queue(op, object) {
      if (!(object instanceof ClientObject) || object.context !== this.ctx) {
        throw new OfficeError("InvalidObjectPath", "The object belongs to another request context.");
      }
      this.ctx._push({ op, target: object._handle });
    }
  }

  function wireUsesHandle(w, handle) {
    if (!w || typeof w !== "object") return false;
    if (w.t === "object") return w.h === handle;
    if (w.t === "array") return Array.isArray(w.v) && w.v.some(item => wireUsesHandle(item, handle));
    return false;
  }

  class RequestContext {
    constructor(transport) {
      this._id = nextContext++; this._queue = []; this._handle = 1; this._loads = new Map(); this._outs = new Map();
      this._transport = transport || root.__dnaOfficeTransport; this._syncing = false; this._running = false;
      this._workbookLive = false; this._workbookQueued = false; this._workbookTracked = false;
      this.workbook = new Workbook(this, this._alloc());
      this.trackedObjects = new TrackedObjects(this);
    }
    _alloc() { return ++this._handle; }
    _ensureWorkbook() {
      if (!this._workbookLive && !this._workbookQueued) {
        this._workbookQueued = true;
        this._queue.push({ op: "root", name: "workbook", out: this.workbook._handle });
      }
    }
    _push(op) {
      if (op.op !== "root" && this.workbook) {
        const wb = this.workbook._handle;
        if (op.target === wb
            || (op.args && op.args.some(a => wireUsesHandle(a, wb)))
            || wireUsesHandle(op.value, wb)) {
          this._ensureWorkbook();
        }
      }
      this._queue.push(op);
      return this._queue.length - 1;
    }
    _discard() {
      this._queue = [];
      this._loads = new Map();
      this._outs = new Map();
      this._workbookQueued = false;
    }
    async _send(id, request) {
      const response = JSON.parse(await this._transport(JSON.stringify({ id, request })));
      if (response.Failed) throw new OfficeError("GeneralException", response.Failed.message);
      if (response.Reply?.id !== id) throw new OfficeError("GeneralException", "Mismatched transport reply.");
      return response.Reply.reply;
    }
    async _release() {
      await this._send(nextMessage++, { AutomationRelease: { context: this._id } });
      if (!this._workbookTracked) this._workbookLive = false;
    }
    async sync() { return this._sync(false); }
    async _sync(final) {
      if (this._syncing) throw new OfficeError("GeneralException", "A sync is already in progress.");
      const ops = this._queue, loads = this._loads, outs = this._outs;
      this._discard();
      if (!ops.length) return final ? this._release() : undefined;
      this._syncing = true;
      try {
        const id = nextMessage++;
        const envelope = await this._send(id, {
          AutomationBatch: { batch_id: id, context: this._id, ops, final_release: final },
        });
        const reply = envelope?.AutomationBatch;
        if (reply?.batch_id !== id) throw new OfficeError("GeneralException", "Mismatched batch reply.");
        const wb = this.workbook._handle;
        for (const r of reply.results) {
          const op = ops[r.ordinal];
          if (op) {
            if (op.op === "root" && op.out === wb) this._workbookLive = true;
            else if (op.op === "track" && op.target === wb) this._workbookTracked = true;
            else if (op.op === "untrack" && op.target === wb) this._workbookTracked = false;
          }
          const obj = loads.get(r.ordinal);
          if (obj) for (const [k, w] of (r.loaded || [])) {
            const v = fromWire(w);
            obj._loaded[k] = TWO_D.has(k) && !Array.isArray(v) ? [[v]] : v;
          }
          if (r.null_object) { const p = outs.get(ops[r.ordinal].out); if (p) p.isNullObject = true; }
        }
        if (final && !this._workbookTracked) this._workbookLive = false;
        if (reply.error) throw new OfficeError(reply.error.code, reply.error.message,
          { errorLocation: "op " + reply.error.ordinal, owner: reply.error.owner });
      } finally { this._syncing = false; }
    }
  }
  const Excel = {
    RequestContext,
    async run(previous, batch) {
      let ctx;
      if (typeof previous === "function") {
        ctx = new RequestContext(batch);
        batch = previous;
      } else if (previous instanceof RequestContext) {
        ctx = previous;
      } else {
        const objects = Array.isArray(previous) ? Array.from(previous) : [previous];
        if (!objects.length || !objects.every(obj => obj instanceof ClientObject)) {
          throw new OfficeError("InvalidArgument", "Expected a request context or previously created Excel objects.");
        }
        ctx = objects[0].context;
        if (!objects.every(obj => obj.context === ctx)) {
          throw new OfficeError("InvalidObjectPath", "The objects belong to different request contexts.");
        }
      }
      if (typeof batch !== "function") throw new OfficeError("InvalidArgument", "Expected a batch function.");
      if (ctx._running || ctx._syncing) throw new OfficeError("GeneralException", "The request context is busy.");
      ctx._running = true;
      try {
        let result;
        try {
          result = await batch(ctx);
        } catch (e) {
          ctx._discard();
          await ctx._release().catch(() => {});
          throw e;
        }
        await ctx._sync(true);
        return result;
      } finally {
        ctx._running = false;
      }
    },
  };
  const actions = new Map();
  const Office = {
    onReady(cb) { const info = { host: "Excel", platform: "PC" }; return cb ? Promise.resolve(cb(info)) : Promise.resolve(info); },
    actions: { associate(name, fn) { actions.set(name, fn); }, _run(name, ev) { return actions.get(name)(ev); } },
    context: { ui: { displayDialogAsync(url, opts, cb) { if (root.__dnaOfficeDialog) return root.__dnaOfficeDialog(url, opts, cb); cb({ status: "failed", error: { code: 12007, message: "No dialog host" } }); } } },
  };
  root.Excel = Excel; root.Office = Office; root.OfficeExtension = { Error: OfficeError };
  if (typeof module !== "undefined") module.exports = { Excel, Office, OfficeExtension: root.OfficeExtension, _wire: { toWire, fromWire } };
})(typeof globalThis !== "undefined" ? globalThis : this);
