// Renders the AgentTx setup panel from the state the extension posts.
// All text is inserted with textContent, never as HTML.
(() => {
  const vscode = acquireVsCodeApi();
  const app = document.getElementById("app");
  const logo = app.dataset.logo;

  const STATUS = {
    connected: { label: "Connected", tone: "ok" },
    "not-connected": { label: "Not connected", tone: "muted" },
    disabled: { label: "Turned off", tone: "warn" },
    "needs-repair": { label: "Needs repair", tone: "warn" },
    error: { label: "Problem", tone: "error" },
  };

  function el(tag, props, ...children) {
    const node = document.createElement(tag);
    for (const [key, value] of Object.entries(props || {})) {
      if (value === undefined || value === null || value === false) continue;
      if (key === "class") node.className = value;
      else if (key === "action") node.dataset.action = value;
      else if (key === "client") node.dataset.id = value;
      else node.setAttribute(key, value === true ? "" : String(value));
    }
    for (const child of children.flat()) {
      if (child === undefined || child === null || child === false) continue;
      node.append(child instanceof Node ? child : String(child));
    }
    return node;
  }

  function button(label, action, options) {
    const { client, primary, disabled } = options || {};
    return el(
      "button",
      { type: "button", class: primary ? "btn primary" : "btn", action, client, disabled },
      label,
    );
  }

  function pill(tone, label) {
    return el("span", { class: `pill ${tone}` }, label);
  }

  function step(number, title, done, ...body) {
    return el(
      "section",
      { class: done ? "step done" : "step" },
      el(
        "h2",
        {},
        el("span", { class: "num", "aria-hidden": "true" }, done ? "✓" : String(number)),
        title,
      ),
      ...body,
    );
  }

  function header() {
    return el(
      "header",
      { class: "brand" },
      el("img", { src: logo, alt: "", width: 32, height: 32 }),
      el(
        "div",
        {},
        el("h1", {}, "AgentTx"),
        el("p", { class: "muted small" }, "Safe, undoable actions for your AI agent."),
      ),
    );
  }

  function programStep(state) {
    const { program, test } = state;
    const found = program.kind === "found";
    const body = [];

    if (program.kind === "checking") {
      body.push(el("p", { class: "muted" }, program.detail));
    } else if (program.kind === "installing") {
      body.push(
        el(
          "p",
          { class: "busy" },
          el("span", { class: "spinner", "aria-hidden": "true" }),
          program.detail,
        ),
      );
    } else if (found) {
      body.push(
        el(
          "div",
          { class: "card ok" },
          el(
            "div",
            { class: "card-head" },
            el("strong", {}, "AgentTx program"),
            pill("ok", `Installed · v${program.version}`),
          ),
          el("p", { class: "mono", title: program.path }, program.displayPath || program.path),
          el(
            "div",
            { class: "actions" },
            button(test && test.kind === "running" ? "Testing…" : "Test AgentTx", "test", {
              disabled: test && test.kind === "running",
            }),
          ),
        ),
      );
      if (test && test.kind !== "running") {
        body.push(el("p", { class: `note ${test.kind === "ok" ? "ok" : "error"}` }, test.detail));
      }
    } else {
      body.push(
        el("p", {}, program.detail),
        el(
          "p",
          { class: "muted small" },
          "It's one small program. The button below downloads it for you and checks the file is intact.",
        ),
        el(
          "div",
          { class: "actions" },
          program.canInstall ? button("Install AgentTx", "install", { primary: true }) : null,
          button("Open the guide", "openGuide"),
        ),
      );
      if (!program.canInstall) {
        body.push(
          el(
            "p",
            { class: "note warn" },
            "There is no ready-made download for this computer yet. The guide shows how to build AgentTx from source.",
          ),
        );
      }
    }
    return step(1, "Install AgentTx", found, ...body);
  }

  function clientCard(client, canConnect) {
    const status = STATUS[client.status] || STATUS.error;
    const actions = [];
    const locked = !canConnect;

    if (client.status === "not-connected") {
      actions.push(
        button(`Connect ${client.label}`, "connect", {
          client: client.id,
          primary: true,
          disabled: locked,
        }),
      );
    } else if (client.status === "needs-repair") {
      actions.push(
        button("Repair", "connect", { client: client.id, primary: true, disabled: locked }),
        button("Disconnect", "disconnect", { client: client.id }),
      );
    } else if (client.status === "disabled") {
      actions.push(
        button("Turn on", "connect", { client: client.id, primary: true, disabled: locked }),
        button("Disconnect", "disconnect", { client: client.id }),
      );
    } else if (client.status === "connected") {
      actions.push(button("Disconnect", "disconnect", { client: client.id }));
    }
    if (client.configPath && client.status !== "not-connected") {
      actions.push(button("Open settings file", "openConfig", { client: client.id }));
    }

    return el(
      "article",
      { class: `card ${status.tone}` },
      el(
        "div",
        { class: "card-head" },
        el("strong", {}, client.label),
        pill(status.tone, status.label),
      ),
      el("p", { class: "muted small" }, client.description),
      client.status === "not-connected"
        ? null
        : el("p", { class: client.status === "connected" ? "mono" : "detail" }, client.detail),
      client.status === "connected" ? el("p", { class: "hint" }, "Next: ", client.hint) : null,
      actions.length > 0 ? el("div", { class: "actions" }, ...actions) : null,
      client.extensionInstalled
        ? null
        : el(
            "p",
            { class: "muted small" },
            `The ${client.label} extension isn't installed in VS Code. `,
            el("a", { href: "#", action: "openExtension", client: client.id }, "Get it"),
          ),
    );
  }

  function clientsStep(state) {
    const canConnect = state.program.kind === "found";
    const anyConnected = state.clients.some((client) => client.status === "connected");
    return step(
      2,
      "Connect your AI agent",
      anyConnected,
      canConnect
        ? el(
            "p",
            { class: "muted small" },
            "Click Connect for the agent you use. You can connect more than one.",
          )
        : el("p", { class: "note muted" }, "Install AgentTx first, then connect your agent here."),
      ...state.clients.map((client) => clientCard(client, canConnect)),
    );
  }

  function tryStep(state) {
    return step(
      3,
      "Try it",
      false,
      el("p", {}, "Open your agent's chat and paste this request:"),
      el(
        "div",
        { class: "request" },
        el("p", {}, state.request),
        button("Copy request", "copyRequest"),
      ),
      el(
        "p",
        { class: "muted small" },
        "AgentTx runs every step. If one fails, it undoes the damage and tells the AI how to continue.",
      ),
    );
  }

  function footer() {
    return el(
      "footer",
      {},
      el("a", { href: "#", action: "openGuide" }, "Step-by-step guide"),
      el("a", { href: "#", action: "refresh" }, "Refresh"),
    );
  }

  function render(state) {
    app.replaceChildren(header(), programStep(state), clientsStep(state), tryStep(state), footer());
  }

  app.addEventListener("click", (event) => {
    const target = event.target instanceof Element ? event.target.closest("[data-action]") : null;
    if (!(target instanceof HTMLElement) || target.hasAttribute("disabled")) return;
    event.preventDefault();
    if (target.dataset.action === "copyRequest") {
      target.textContent = "Copied ✓";
      setTimeout(() => (target.textContent = "Copy request"), 2000);
    }
    vscode.postMessage({ type: target.dataset.action, id: target.dataset.id });
  });

  window.addEventListener("message", (event) => {
    if (event.data && event.data.type === "state") render(event.data.state);
  });

  vscode.postMessage({ type: "ready" });
})();
