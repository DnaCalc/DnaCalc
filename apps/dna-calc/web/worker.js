// DNA Calc's calculation worker (dc-p96): a module worker that loads the same glue and wasm as
// the page, from the same asset folder. The wasm's start function sees no window and installs the
// worker's message handler (dna_calc::worker), which then tells the page it has started.
import init from "./dna-calc-web.js";

init({ module_or_path: new URL("./dna-calc-web_bg.wasm", import.meta.url) }).catch(function (error) {
  self.postMessage(JSON.stringify({ Failed: { id: null, message: "the worker did not load: " + error } }));
});
