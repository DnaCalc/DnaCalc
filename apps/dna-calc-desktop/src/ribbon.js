// WebView2 transport and rendering for the host's custom workbook ribbon buttons.
(() => {
    const host = window.chrome?.webview;
    if (!host || window !== window.top) return;

    let ribbon;
    let buttons = [];
    const awaitWorkspace = new MutationObserver(render);

    function render() {
        if (!buttons.length) {
            awaitWorkspace.disconnect();
            ribbon?.remove();
            return;
        }
        const workspace = document.getElementById("dc-workspace");
        if (!workspace) {
            awaitWorkspace.observe(document, { childList: true, subtree: true });
            return;
        }
        awaitWorkspace.disconnect();
        if (!ribbon) {
            ribbon = document.createElement("nav");
            ribbon.className = "dc-page-toolbar";
            ribbon.setAttribute("aria-label", "COM add-in commands");
        }
        if (!ribbon.isConnected) workspace.before(ribbon);
        const active = document.activeElement?.dataset?.ribbonToken;
        const groups = new Map();
        ribbon.replaceChildren();
        for (const button of buttons) {
            const name = [button.tab, button.group].filter(Boolean).join(" / ");
            if (!groups.has(name)) {
                const group = document.createElement("div");
                group.setAttribute("role", "group");
                if (name) group.setAttribute("aria-label", name);
                groups.set(name, group);
                ribbon.append(group);
            }
            const control = document.createElement("button");
            control.type = "button";
            control.textContent = button.label;
            control.disabled = !button.enabled;
            control.dataset.ribbonToken = button.token;
            control.addEventListener("click", () => host.postMessage("ribbon:" + button.token));
            groups.get(name).append(control);
            if (active === button.token) control.focus();
        }
    }

    host.addEventListener("message", ({ data }) => {
        if (typeof data !== "string" || !data.startsWith("ribbon:")) return;
        buttons = JSON.parse(data.slice(7));
        render();
    });
})();
