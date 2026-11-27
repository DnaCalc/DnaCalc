import init from "./dna-calc-preview-worker-web.js";

const pending = [];
const queue = event => pending.push(event);
self.addEventListener("message", queue);
init({ module_or_path: new URL("./dna-calc-preview-worker-web_bg.wasm", import.meta.url) }).then(() => {
  self.removeEventListener("message", queue);
  for (const event of pending) self.onmessage(event);
}).catch(error => {
  self.postMessage(JSON.stringify({ schema: 1, request: 0, message: { Failure: { Transport: String(error) } } }));
});
