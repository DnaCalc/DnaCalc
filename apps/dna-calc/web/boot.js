// DNA Calc's start: the fault record, then the app. A module of its own beside the wasm-bindgen
// output, so the page needs no inline script and runs under a strict script-src (dc-ysl.3).
//
// Every uncaught error, unhandled rejection and Content-Security-Policy violation is recorded, so
// a trap the app cannot report itself (a panic in a reactive effect shows only as
// "RuntimeError: unreachable") is still seen by the browser smoke test
// (`cargo xtask app smoke calc`), which reads this list.
window.__dnacalcFaults = window.__dnacalcFaults || [];
var faults = window.__dnacalcFaults;
window.addEventListener("error", function (event) {
  faults.push("error: " + (event.error && event.error.stack ? event.error.stack : event.message));
});
window.addEventListener("unhandledrejection", function (event) {
  var reason = event.reason;
  faults.push("unhandledrejection: " + (reason && reason.stack ? reason.stack : String(reason)));
});
document.addEventListener("securitypolicyviolation", function (event) {
  faults.push("csp: " + event.violatedDirective + " blocked " + (event.blockedURI || "inline") +
    " at " + event.sourceFile + ":" + event.lineNumber);
});

function loading() {
  return document.getElementById("dnacalc-loading");
}

// The glue is imported only after the listeners are in place, relative to this file, so the page
// may put both in an asset folder of their own.
import("./dna-calc-web.js")
  .then(function (glue) {
    return glue.default({ module_or_path: new URL("./dna-calc-web_bg.wasm", import.meta.url) });
  })
  .then(function () {
    var element = loading();
    if (element) { element.remove(); }
  })
  .catch(function (error) {
    faults.push("load: " + (error && error.stack ? error.stack : String(error)));
    var element = loading();
    if (element) { element.textContent = "DNA Calc did not load: " + error; }
  });
