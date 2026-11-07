// The desktop shell's one page-side piece: under WebView2 the page's "Worker" is the native
// workbook worker. The shell injects this before the page runs; it is transport only, a carrier
// for the same JSON texts and tile bytes the browser worker exchanges. It holds no behaviour: the
// page, the protocol and the server are unchanged, and a browser (which has no `chrome.webview`)
// never runs it.
//
// Framing, both ways: a message with no bytes is its JSON text; a message with bytes is
// `text U+0001 base64(bytes)`. JSON text escapes every control character, so the first U+0001 is the
// split (and, unlike NUL, it survives the host's string passing).
(function () {
  "use strict";
  var host = window.chrome && window.chrome.webview;
  if (!host) { return; }
  var NativeWorker = window.Worker;
  var SEPARATOR = "\u0001";

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
    this.onmessage = null;
    this.onerror = null;
    this._listeners = [];
    host.addEventListener("message", function (event) {
      var data = event.data;
      var split = data.indexOf(SEPARATOR);
      var message = split < 0 ? data : [data.slice(0, split), decode(data.slice(split + 1)).buffer];
      var delivered = { data: message };
      if (self.onmessage) { self.onmessage(delivered); }
      self._listeners.forEach(function (listener) { listener(delivered); });
    });
    // The shell answers with Started once it listens, as the browser worker does.
    host.postMessage("hello");
  }
  DesktopWorker.prototype.postMessage = function (data) {
    if (typeof data === "string") {
      host.postMessage(data);
    } else {
      host.postMessage(data[0] + SEPARATOR + encode(new Uint8Array(data[1])));
    }
  };
  DesktopWorker.prototype.addEventListener = function (type, listener) {
    if (type === "message") { this._listeners.push(listener); }
  };
  DesktopWorker.prototype.terminate = function () {};
  window.Worker = DesktopWorker;
  window.__dnacalcDesktop = { native: NativeWorker !== undefined };
})();
