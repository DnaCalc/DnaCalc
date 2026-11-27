const assert = require("node:assert/strict");
const { readFileSync } = require("node:fs");
const { runInNewContext } = require("node:vm");
const test = require("node:test");

test("shared tile delivery preserves order, channel and bytes before releasing the buffer", () => {
  const listeners = new Map();
  const fire = (type, event) => {
    for (const listener of listeners.get(type) || []) listener(event);
  };
  const posted = [];
  const buffer = new Uint8Array([0, 7, 255]).buffer;
  let releases = 0;
  const host = {
    addEventListener(type, listener) {
      if (!listeners.has(type)) listeners.set(type, new Set());
      listeners.get(type).add(listener);
    },
    removeEventListener(type, listener) { listeners.get(type)?.delete(listener); },
    postMessage(message) { posted.push(message); },
    releaseBuffer(released) { assert.equal(released, buffer); releases++; }
  };
  const window = { chrome: { webview: host }, Worker: function () {} };
  runInNewContext(readFileSync(require.resolve("../web/desktop-bridge.js"), "utf8"), {
    window, document: { currentScript: null }, Uint8Array
  });

  const first = new window.Worker("worker.js");
  const second = new window.Worker("worker.js");
  const received = [];
  first.onmessage = event => received.push(event.data);
  second.onmessage = event => {
    const [control, bytes] = event.data;
    assert.equal(releases, 0);
    received.push([control, Array.from(new Uint8Array(bytes))]);
  };

  fire("message", { data: "ready" });
  fire("sharedbufferreceived", { additionalData: { text: "@1\u0002tiles" }, getBuffer: () => buffer });
  assert.deepEqual(posted, ["hello", "@1\u0002hello"]);
  assert.deepEqual(received, ["ready", ["tiles", [0, 7, 255]]]);
  assert.equal(releases, 1);
});
