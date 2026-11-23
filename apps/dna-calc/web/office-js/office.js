/* DNA Calc clean-room Office.js-compatible client (MIT; dc-17spk.5). Written from the public API
   surface only; no Microsoft office.js code. Speaks the batch protocol of crates/calc-office-js:
   one message per context.sync(). Transport: globalThis.__dnaOfficeTransport(requestJson) -> Promise<replyJson>. */
(function (root) {
  "use strict";
  let nextBatch = 1, nextContext = 1;

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
    if (v === null || v === undefined) return { t: "null" };
    if (typeof v === "number") return { t: "r8", bits: r8Bits(v) };
    if (typeof v === "boolean") return { t: "bool", v };
    if (typeof v === "string") return { t: "text", v };
    if (v instanceof ClientObject) return { t: "object", h: v._handle };
    if (Array.isArray(v)) {
      const rows = v.length, cols = rows ? v[0].length : 0;
      return { t: "array", rows, cols, v: v.flat().map((x) => toWire(x, ctx)) };
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
      case "text16": return String.fromCharCode(...w.units);
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
      this.context = context; this._handle = handle; this._loaded = {}; this.isNullObject = false;
    }
    _step(name, args, Cls) {
      const out = this.context._alloc();
      this.context._push({ op: "call", target: this._handle, name, args: (args || []).map((a) => toWire(a)), out });
      const o = new (Cls || ClientObject)(this.context, out);
      this.context._outs.set(out, o);
      return o;
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
    _set(name, v) { this.context._push({ op: "set", target: this._handle, name, value: toWire(v) }); }
    toJSON() { return Object.assign({}, this._loaded); }
  }
  function props(Cls, names, writable) {
    for (const n of names) Object.defineProperty(Cls.prototype, n, {
      get() { return this._prop(n); },
      set: writable.includes(n) ? function (v) { this._set(n, v); } : undefined,
    });
  }

  class Range extends ClientObject { calculate() { this._step("calculate"); } clear() { this._step("clear"); } }
  props(Range, ["values", "formulas", "numberFormat", "text", "address", "rowIndex", "columnIndex"], ["values", "formulas", "numberFormat"]);
  class Worksheet extends ClientObject {
    getRange(a) { return this._step("getRange", [a], Range); }
    getUsedRangeOrNullObject() { return this._step("getUsedRangeOrNullObject", [], Range); }
    calculate() { this._step("calculate"); }
    activate() { this._step("activate"); }
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
  class Application extends ClientObject { calculate() { this._step("calculate"); } }

  class TrackedObjects {
    constructor(ctx) { this.ctx = ctx; }
    add(o) { this.ctx._push({ op: "track", target: o._handle }); }
    remove(o) { this.ctx._push({ op: "untrack", target: o._handle }); }
  }

  class RequestContext {
    constructor(transport) {
      this._id = nextContext++; this._queue = []; this._handle = 1; this._loads = new Map(); this._outs = new Map();
      this._transport = transport || root.__dnaOfficeTransport; this._syncing = false;
      this._push({ op: "root", name: "workbook", out: this._alloc() });
      this.workbook = new Workbook(this, this._handle);
      this.trackedObjects = new TrackedObjects(this);
    }
    _alloc() { return ++this._handle; }
    _push(op) { this._queue.push(op); return this._queue.length - 1; }
    async sync(final) {
      if (this._syncing) throw new OfficeError("GeneralException", "A sync is already in progress.");
      const ops = this._queue, loads = this._loads, outs = this._outs;
      this._queue = []; this._loads = new Map(); this._outs = new Map();
      if (!ops.length && !final) return;
      this._syncing = true;
      try {
        const reply = JSON.parse(await this._transport(JSON.stringify({ batch_id: nextBatch++, context: this._id, ops, final_release: !!final })));
        for (const r of reply.results) {
          const obj = loads.get(r.ordinal);
          if (obj) for (const [k, w] of r.loaded) {
            const v = fromWire(w);
            obj._loaded[k] = TWO_D.has(k) && !Array.isArray(v) ? [[v]] : v;
          }
          if (r.null_object) { const p = outs.get(ops[r.ordinal].out); if (p) p.isNullObject = true; }
        }
        if (reply.error) throw new OfficeError(reply.error.code, reply.error.message,
          { errorLocation: "op " + reply.error.ordinal, owner: reply.error.owner });
      } finally { this._syncing = false; }
    }
  }
  const Excel = {
    RequestContext,
    async run(batch, transport) {
      const ctx = typeof batch === "function" ? new RequestContext(transport) : new RequestContext(batch);
      const fn = typeof batch === "function" ? batch : arguments[1];
      let result;
      try { result = await fn(ctx); } catch (e) { ctx._queue = []; await ctx.sync(true).catch(() => {}); throw e; }
      await ctx.sync(true);
      return result;
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
