window.__dnacalcPreviewFaults = [];
window.addEventListener("error", event => window.__dnacalcPreviewFaults.push(event.message));
window.addEventListener("unhandledrejection", event => window.__dnacalcPreviewFaults.push(String(event.reason)));
document.addEventListener("securitypolicyviolation", event => window.__dnacalcPreviewFaults.push(event.violatedDirective));
window.__dnaClassicWorker = new URL("./worker.js", import.meta.url).href;
try {
  const theme = localStorage.getItem("dna-theme");
  if (theme) document.documentElement.setAttribute("data-dna-theme", theme);
} catch {}
import("./dna-calc-preview-web.js")
  .then(glue => glue.default({ module_or_path: new URL("./dna-calc-preview-web_bg.wasm", import.meta.url) }))
  .catch(error => {
    window.__dnacalcPreviewFaults.push(String(error));
    const loading = document.getElementById("dna-classic-loading");
    if (loading) loading.textContent = "Classic did not load.";
  });
