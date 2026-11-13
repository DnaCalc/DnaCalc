// Native-only: Node's VM exercises the page carrier with WebView2 and WebSocket host objects.
// Run: node --test apps/dnacalc/tests/bridge.cjs
const { test } = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const path = require('node:path');
const script = fs.readFileSync(path.join(__dirname, '../../dna-calc/web/desktop-bridge.js'), 'utf8');

function page(carrier) {
  const sent = [], listeners = {};
  const host = {
    postMessage: text => sent.push(text),
    addEventListener: (kind, f) => (listeners[kind] ??= []).push(f),
    removeEventListener: (kind, f) => { listeners[kind] = listeners[kind].filter(x => x !== f); }
  };
  const emit = (kind, data) => (listeners[kind] ?? []).slice().forEach(f => f({data}));
  class Socket {
    static CONNECTING = 0; static OPEN = 1;
    constructor(url) { this.readyState = 0; this.url = url; Socket.last = this; }
    send(text) { sent.push(text); }
    addEventListener(...args) { host.addEventListener(...args); }
    removeEventListener(...args) { host.removeEventListener(...args); }
  }
  class ErrorEvent {
    constructor(type, options) { this.type = type; Object.assign(this, options); }
    preventDefault() { this.defaultPrevented = true; }
  }
  const window = {
    Worker: function OriginalWorker() {},
    chrome: carrier === 'webview' ? {webview: host} : undefined,
    location: {href: 'http://127.0.0.1:1234/test-capability/'},
    btoa: s => Buffer.from(s, 'binary').toString('base64'),
    atob: s => Buffer.from(s, 'base64').toString('binary')
  };
  const context = { window, WebSocket: Socket, ErrorEvent, URL, Uint8Array,
    document: {currentScript: {getAttribute: () => carrier === 'socket' ? 'socket' : null}} };
  vm.runInNewContext(script, context);
  return { window, sent, emit, Socket };
}

for (const carrier of ['webview', 'socket']) {
  test(`${carrier} preserves Worker channels and text/binary framing`, () => {
    const {window, sent, emit, Socket} = page(carrier);
    const first = new window.Worker('worker.js'), second = new window.Worker('worker.js');
    if (carrier === 'socket') {
      assert.equal(sent.length, 0);
      assert.equal(Socket.last.url, 'ws://127.0.0.1:1234/test-capability/socket');
      Socket.last.readyState = Socket.OPEN;
      emit('open');
      assert.equal(window.__dnacalcDesktop, undefined);
    } else { assert.equal(window.__dnacalcDesktop.native, true); }
    assert.deepEqual(sent, ['hello', '@1\u0002hello']);
    first.postMessage('request');
    second.postMessage(['upload', new Uint8Array([0, 1, 128, 255]).buffer]);
    assert.deepEqual(sent.slice(2), ['request', '@1\u0002upload\u0001AAGA/w==']);
    let firstMessage, secondMessage, listenerMessage;
    first.onmessage = e => { firstMessage = e.data; };
    second.onmessage = e => { secondMessage = e.data; };
    second.addEventListener('message', e => { listenerMessage = e.data; });
    emit('message', 'reply');
    emit('message', '@1\u0002tiles\u0001AAGA/w==');
    assert.equal(firstMessage, 'reply');
    assert.equal(secondMessage[0], 'tiles');
    assert.deepEqual([...new Uint8Array(secondMessage[1])], [0, 1, 128, 255]);
    assert.equal(secondMessage, listenerMessage);
    second.terminate();
    if (carrier === 'socket') {
      assert.equal(sent.at(-1), '@1\u0002bye');
      emit('message', '@1\u0002later');
      assert.equal(secondMessage[0], 'tiles');
      let error;
      first.onerror = e => { e.preventDefault(); error = e.message; };
      emit('close');
      assert.match(error, /connection closed/);
      Socket.last.readyState = 3;
      assert.doesNotThrow(() => first.terminate());
    }
  });
}

test('a regular browser retains its original Worker', () => {
  const {window} = page('browser');
  assert.equal(window.Worker.name, 'OriginalWorker');
  assert.equal(window.__dnacalcDesktop, undefined);
});
