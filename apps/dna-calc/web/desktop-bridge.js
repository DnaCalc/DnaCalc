// The desktop shell's one page-side piece: under WebView2 the page's "Worker" is the native
// workbook worker. The shell injects this before the page runs; it is transport only, a carrier
// for the same JSON texts and tile bytes the browser worker exchanges. It holds no behaviour: the
// page, the protocol and the server are unchanged. `dnacalc serve` loads this same script with a
// data-dnacalc-socket attribute to select its loopback WebSocket carrier.
//
// WebView2 sends host-to-page tiles in a read-only shared buffer with the control text as metadata.
// Text-only controls stay strings. The older runtime and loopback carriers use
// `text U+0001 base64(bytes)`; uploads also use that string route.
(function () {
  "use strict";
  var host = window.chrome && window.chrome.webview;
  var socketPath = document.currentScript && document.currentScript.getAttribute("data-dnacalc-socket");
  var socketCarrier = !host && socketPath;
  if (socketCarrier) {
    var address = new URL(socketPath, window.location.href);
    address.protocol = "ws:";
    var socket = new WebSocket(address.href);
    var waiting = [];
    socket.addEventListener("open", function () {
      waiting.forEach(function (message) { socket.send(message); });
      waiting = [];
    });
    host = {
      postMessage: function (message) {
        if (socket.readyState === WebSocket.CONNECTING) { waiting.push(message); }
        else if (socket.readyState === WebSocket.OPEN) { socket.send(message); }
        else { throw new Error("The native workbook connection is closed"); }
      },
      addEventListener: function (type, listener) { socket.addEventListener(type, listener); },
      removeEventListener: function (type, listener) { socket.removeEventListener(type, listener); }
    };
  }
  if (!host) { return; }
  // The host may ask for print requests to be answered without the native print dialog (it blocks
  // the page until a person closes it). This script also runs in the print frame.
  if (window.__dnacalcNoPrintUi) {
    window.print = function () {};
  }
  var NativeWorker = window.Worker;
  var SEPARATOR = "\u0001";
  // Each Worker the page starts is a worker of its own, as in a browser. The first (the page's)
  // is channel 0 and its messages are unprefixed; a later one is channel N and its messages carry
  // `@N` and U+0002 in front, both ways.
  var CHANNEL_MARK = "\u0002";
  var channels = 0;
  var receivers = new Map();

  function channelOf(text) {
    var mark = text.charAt(0) === "@" ? text.indexOf(CHANNEL_MARK) : -1;
    return mark < 0 ? 0 : parseInt(text.slice(1, mark), 10);
  }

  if (!socketCarrier) {
    host.addEventListener("sharedbufferreceived", function (event) {
      var buffer = event.getBuffer();
      try {
        var text = event.additionalData && event.additionalData.text;
        if (typeof text !== "string") { return; }
        var receive = receivers.get(channelOf(text));
        if (receive) {
          // Callers can retain or transfer their copy. The WebView allocation is read-only.
          receive({ data: text }, buffer.slice(0));
        }
      } finally {
        host.releaseBuffer(buffer);
      }
    });
  }

  function encode(bytes) {
    var text = "";
    for (var at = 0; at < bytes.length; at += 0x8000) {
      text += String.fromCharCode.apply(null, bytes.subarray(at, at + 0x8000));
    }
    return window.btoa(text);
  }

  function decode(text) {
    var binary = window.atob(text);
    var bytes = new Uint8Array(binary.length);
    for (var at = 0; at < binary.length; at += 1) { bytes[at] = binary.charCodeAt(at); }
    return bytes;
  }

  function DesktopWorker(url, options) {
    var self = this;
    var channel = channels++;
    var prefix = channel === 0 ? "" : "@" + channel + CHANNEL_MARK;
    this._prefix = prefix;
    this._onmessage = null;
    this._onerror = null;
    this._listeners = [];
    this._receive = function (event, bytes) {
      var data = event.data;
      if (typeof data !== "string" || (!socketCarrier && data.startsWith("ribbon:"))) { return; }
      if (channelOf(data) !== channel) { return; }
      if (channel !== 0) { data = data.slice(data.indexOf(CHANNEL_MARK) + 1); }
      var split = data.indexOf(SEPARATOR);
      var message = bytes ? [data, bytes] :
        split < 0 ? data : [data.slice(0, split), decode(data.slice(split + 1)).buffer];
      var delivered = { data: message };
      // The handler slot, as a browser's event dispatch reads it: a page script that wraps the
      // `onmessage` accessor (the smoke suite's latency page does) still sees every message.
      if (self._onmessage) { self._onmessage.call(self, delivered); }
      self._listeners.forEach(function (listener) { listener(delivered); });
    };
    this._channel = channel;
    receivers.set(channel, this._receive);
    host.addEventListener("message", this._receive);
    if (socketCarrier) {
      this._disconnected = function () {
        var event = new ErrorEvent("error", {
          message: "The native workbook connection closed; reload the page to reconnect",
          cancelable: true
        });
        if (self._onerror) { self._onerror.call(self, event); }
      };
      host.addEventListener("close", this._disconnected);
      host.addEventListener("error", this._disconnected);
    }
    // The shell answers with Started once it listens, as the browser worker does.
    host.postMessage(prefix + "hello");
  }
  // `onmessage` and `onerror` are accessors on the prototype, as on a browser Worker, so a subclass
  // may override them and reach this slot through `super`.
  ["onmessage", "onerror"].forEach(function (name) {
    Object.defineProperty(DesktopWorker.prototype, name, {
      configurable: true,
      enumerable: true,
      get: function () { return this["_" + name]; },
      set: function (handler) { this["_" + name] = typeof handler === "function" ? handler : null; }
    });
  });
  DesktopWorker.prototype.postMessage = function (data) {
    if (typeof data === "string") {
      host.postMessage(this._prefix + data);
    } else {
      host.postMessage(this._prefix + data[0] + SEPARATOR + encode(new Uint8Array(data[1])));
    }
  };
  DesktopWorker.prototype.addEventListener = function (type, listener) {
    if (type === "message") { this._listeners.push(listener); }
  };
  DesktopWorker.prototype.terminate = function () {
    receivers.delete(this._channel);
    host.removeEventListener("message", this._receive);
    if (socketCarrier) {
      if (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING) {
        host.postMessage(this._prefix + "bye");
      }
      host.removeEventListener("close", this._disconnected);
      host.removeEventListener("error", this._disconnected);
    }
  };
  window.Worker = DesktopWorker;
  // The desktop marker selects native file dialogs in the page. The socket route uses the
  // browser's file input and downloads, even though its workbook worker is native.
  if (!socketCarrier) { window.__dnacalcDesktop = { native: NativeWorker !== undefined }; }
})();
