// Separate workbook worker entry; the page module contains no workbook implementation.
import init from "./dna-calc-worker-web.js";

init({ module_or_path: new URL("./dna-calc-worker-web_bg.wasm", import.meta.url) }).catch(function (error) {
  self.postMessage(JSON.stringify({ Failed: { id: null, message: "the worker did not load: " + error } }));
});
