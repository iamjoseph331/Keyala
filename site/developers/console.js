"use strict";

const node = (id) => document.getElementById(id);
const apiInput = node("api-url");
const registerForm = node("register-form");
const reportsForm = node("reports-form");
const tokenInput = node("app-token");
const periodInput = node("report-days");
const status = node("console-status");
const rotateButton = node("rotate-token");
const rotateDialog = node("rotate-confirmation");
const hostedServers = new Set([
  "https://masterkey-prod-vkgm.onrender.com",
  "https://masterkey-staging-vkgm.onrender.com",
]);
const regionNames = new Intl.DisplayNames(["en"], { type: "region" });
const money = new Intl.NumberFormat("en-US", {
  style: "currency", currency: "USD", maximumFractionDigits: 6,
});
const integer = new Intl.NumberFormat("en-US");
const dateLabel = new Intl.DateTimeFormat("en-US", {
  month: "short", day: "numeric", year: "numeric", timeZone: "UTC",
});
const loopback = new Set(["localhost", "127.0.0.1", "[::1]"]);
const isToken = (value) => /^mk_app_[A-Za-z0-9_-]+$/.test(value);

let created = null;
let activeReport = null;
let request = null;
let registration = null;
let activeApp = null;

function message(element, text, kind = "info") {
  if (!element) return;
  element.textContent = text;
  element.dataset.kind = kind;
  element.hidden = !text;
}

function serverOrigin() {
  const url = new URL(apiInput.value.trim());
  if (
    (url.protocol !== "https:" && !(url.protocol === "http:" && loopback.has(url.hostname))) ||
    url.username || url.password || url.search || url.hash || url.pathname !== "/"
  ) {
    throw new Error("Enter an HTTPS server origin or an HTTP localhost address, without a path.");
  }
  return url.origin;
}

function registrationPayload() {
  const name = node("app-name").value.trim();
  const homepage = node("app-homepage").value.trim();
  const origins = node("app-origins").value.split(",").map((origin) => origin.trim()).filter(Boolean);
  if (!name || new TextEncoder().encode(name).length > 100) {
    throw new Error("Enter an app name of up to 100 bytes.");
  }
  const payload = { name };
  if (homepage) {
    const url = new URL(homepage);
    if (!["https:", "http:"].includes(url.protocol) || url.username || url.password) {
      throw new Error("Enter an HTTP or HTTPS homepage without credentials.");
    }
    payload.homepage = url.href;
  }
  if (origins.length) {
    if (origins.length > 10) throw new Error("Enter up to 10 browser origins.");
    payload.allowed_origins = [...new Set(origins.map((origin) => {
      const url = new URL(origin);
      if (
        (url.protocol !== "https:" && !(url.protocol === "http:" && loopback.has(url.hostname))) ||
        url.pathname !== "/" || url.search || url.hash || url.username || url.password
      ) {
        throw new Error("Each browser origin must use HTTPS or HTTP on localhost, with no path.");
      }
      return url.origin;
    }))];
  }
  return payload;
}

async function call(origin, path, options) {
  let response;
  try {
    response = await fetch(origin + path, {
      ...options,
      credentials: "omit",
      redirect: "error",
      cache: "no-store",
      referrerPolicy: "no-referrer",
    });
  } catch (error) {
    if (error.name === "AbortError") throw error;
    throw new Error("Could not reach the MasterKey API from this page. Check the server URL and console access. The tutorials include a terminal quickstart.");
  }
  const body = await response.json().catch(() => null);
  if (!response.ok) {
    if (response.status === 404 && path.startsWith("/v1/app")) {
      throw new Error("This server does not expose developer reports yet. Check the server version.");
    }
    if (response.status === 401) throw new Error("The app token was rejected. Check that it belongs to this server.");
    throw new Error(typeof body?.error?.message === "string" ? body.error.message : `MasterKey returned HTTP ${response.status}.`);
  }
  if (!body || typeof body !== "object") throw new Error("The server returned an unreadable response.");
  return body;
}

function setCredential(id, value) {
  const element = node(id);
  if (element instanceof HTMLInputElement) element.value = value;
  else element.textContent = value;
}

function showCreatedCredentials(reveal = false) {
  const tokenOutput = node("created-token");
  const toggle = node("show-token");
  setCredential("created-id", created?.app_id ?? "");
  if (tokenOutput instanceof HTMLInputElement) {
    tokenOutput.type = reveal ? "text" : "password";
    setCredential("created-token", created?.app_secret ?? "");
  } else {
    setCredential("created-token", reveal ? created?.app_secret ?? "" : "••••••••••••");
  }
  toggle.textContent = reveal ? "Hide" : "Show";
  toggle.setAttribute("aria-pressed", String(reveal));
  node("creation-result").hidden = !created;
}

function setPending(form, busy) {
  form.querySelector('button[type="submit"]').disabled = busy;
  form.setAttribute("aria-busy", String(busy));
}

function parseReport(data, app) {
  const counts = ["requests", "completed", "denied", "failed", "uncertain", "unmetered",
    "users", "new_users", "input_tokens", "output_tokens"];
  const count = (value) => Number.isSafeInteger(value) && value >= 0;
  const cost = (value) => Number.isFinite(value) && value >= 0;
  const date = (value) => typeof value === "string" && /^\d{4}-\d{2}-\d{2}$/.test(value) && Number.isFinite(Date.parse(value));
  const table = (value, validate) => Array.isArray(value) && value.every((row) =>
    row && count(row.requests) && cost(row.cost_usd) && validate(row));
  if (
    typeof app.app_id !== "string" || typeof app.name !== "string" || data.app_id !== app.app_id ||
    !date(data.from) || !date(data.to) || Date.parse(data.from) > Date.parse(data.to) ||
    !counts.every((field) => count(data.totals?.[field])) ||
    !cost(data.totals?.cost_usd) || !cost(data.totals?.estimated_usd) ||
    !table(data.daily, (row) => date(row.date)) ||
    !table(data.countries, (row) => row.country === null || typeof row.country === "string") ||
    !table(data.models, (row) => typeof row.vendor === "string" && typeof row.model === "string")
  ) throw new Error("The server returned a report this console could not read.");
  return data;
}

function fillRows(id, values) {
  const list = node(id);
  if (!values.length) {
    const tr = document.createElement("tr");
    const td = document.createElement("td");
    td.colSpan = 3;
    td.textContent = "No activity in this range.";
    tr.append(td);
    list.replaceChildren(tr);
    return;
  }
  list.replaceChildren(...values.map((columns) => {
    const row = document.createElement("tr");
    columns.forEach((value, index) => {
      const cell = document.createElement(index === 0 ? "th" : "td");
      if (index === 0) cell.scope = "row";
      cell.textContent = value;
      row.append(cell);
    });
    return row;
  }));
}

function drawChart(daily) {
  const chart = node("report-chart");
  const peak = Math.max(1, ...daily.map((row) => row.requests));
  chart.replaceChildren(...daily.map((row) => {
    const bar = document.createElement("div");
    bar.className = "chart-bar";
    bar.style.setProperty("--height", `${(100 * row.requests) / peak}%`);
    bar.title = `${row.date}: ${integer.format(row.requests)} requests`;
    return bar;
  }));
  chart.hidden = !daily.length;
}

function renderReport(data, name, mode) {
  const { totals } = data;
  const daily = [...data.daily].sort((a, b) => a.date.localeCompare(b.date));
  const rows = (items, label) => items.map((item) =>
    [label(item), integer.format(item.requests), money.format(item.cost_usd)]);
  node("report-mode").textContent = mode === "sample" ? "Sample data" : "Live report";
  node("report-title").textContent = name;
  node("report-period").textContent =
    `${dateLabel.format(new Date(data.from))} through ${dateLabel.format(new Date(data.to))} (UTC)`;
  node("total-requests").textContent = integer.format(totals.requests);
  node("total-completed").textContent = integer.format(totals.completed);
  node("total-cost").textContent = money.format(totals.cost_usd);
  node("total-unpriced").textContent = integer.format(totals.unmetered);
  const extras = {
    "total-estimate": money.format(totals.estimated_usd),
    "total-denied": integer.format(totals.denied),
    "total-failed": integer.format(totals.failed),
    "total-uncertain": integer.format(totals.uncertain),
    "total-users": integer.format(totals.users),
    "total-new-users": integer.format(totals.new_users),
    "total-input-tokens": integer.format(totals.input_tokens),
    "total-output-tokens": integer.format(totals.output_tokens),
  };
  Object.entries(extras).forEach(([id, value]) => { if (node(id)) node(id).textContent = value; });
  fillRows("daily-rows", rows(daily, (item) => dateLabel.format(new Date(item.date))));
  fillRows("country-rows", rows(data.countries, (item) =>
    /^[A-Z]{2}$/.test(item.country) ? regionNames.of(item.country) : "Unknown"));
  fillRows("model-rows", rows(data.models, (item) => `${item.vendor} / ${item.model}`));
  drawChart(daily);
  node("report").hidden = false;
  rotateButton.hidden = mode !== "live";
  activeReport = mode;
}

function sampleReport(days) {
  const day = 86_400_000;
  const to = Date.UTC(2026, 10, 15);
  const from = to - days * day;
  const daily = Array.from({ length: days }, (_, index) => {
    const requests = (index * 17 + 11) % 35;
    return { date: new Date(from + index * day).toISOString().slice(0, 10), requests, cost_usd: requests * 0.000185 };
  });
  const requests = daily.reduce((sum, row) => sum + row.requests, 0);
  const actual = requests * 0.000185;
  const us = Math.floor(requests * 0.52);
  const jp = Math.floor(requests * 0.30);
  const openAI = Math.floor(requests * 0.68);
  return {
    from: new Date(from).toISOString().slice(0, 10),
    to: new Date(to - day).toISOString().slice(0, 10),
    totals: {
      requests, completed: requests - 6, denied: 3, failed: 2, uncertain: 1,
      cost_usd: actual, estimated_usd: actual * 1.4, unmetered: 2,
      users: Math.ceil(requests / 4), new_users: Math.ceil(requests / 12),
      input_tokens: requests * 112, output_tokens: requests * 32,
    },
    daily,
    countries: [
      { country: "US", requests: us, cost_usd: us * 0.000185 },
      { country: "JP", requests: jp, cost_usd: jp * 0.000185 },
      { country: null, requests: requests - us - jp, cost_usd: (requests - us - jp) * 0.000185 },
    ],
    models: [
      { vendor: "openai", model: "gpt-4o-mini", requests: openAI, cost_usd: openAI * 0.000185 },
      { vendor: "anthropic", model: "claude-haiku-5-5", requests: requests - openAI, cost_usd: (requests - openAI) * 0.000185 },
    ],
  };
}

function clearReport() {
  request?.abort();
  request = null;
  setPending(reportsForm, false);
  node("report").hidden = true;
  activeReport = null;
  activeApp = null;
}

async function loadReport() {
  clearReport();
  const controller = new AbortController();
  request = controller;
  setPending(reportsForm, true);
  message(status, "Loading usage report…");
  try {
    const origin = serverOrigin();
    const token = tokenInput.value.trim();
    if (!isToken(token)) throw new Error("Enter a MasterKey app token beginning with mk_app_.");
    const options = { headers: { Authorization: `Bearer ${token}` }, signal: controller.signal };
    const [app, data] = await Promise.all([
      call(origin, "/v1/app", options),
      call(origin, `/v1/app/report?days=${encodeURIComponent(periodInput.value)}`, options),
    ]);
    if (request !== controller) return;
    renderReport(parseReport(data, app), app.name, "live");
    activeApp = app;
    message(status, "Usage report loaded.");
  } catch (error) {
    if (request === controller) message(status, error.message, "error");
  } finally {
    if (request === controller) {
      request = null;
      setPending(reportsForm, false);
    }
  }
}

registerForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  if (registration || rotateButton.disabled) return;
  const controller = new AbortController();
  registration = controller;
  const output = node("creation-error") ?? status;
  message(output, "Creating app…");
  setPending(registerForm, true);
  try {
    const data = await call(serverOrigin(), "/v1/apps", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(registrationPayload()),
      signal: controller.signal,
    });
    if (registration !== controller) return;
    if (typeof data.app_id !== "string" || !isToken(data.app_secret)) {
      throw new Error("The server did not return valid app credentials.");
    }
    created = data;
    showCreatedCredentials();
    clearReport();
    tokenInput.value = data.app_secret;
    message(output, "App created. Save the token before you leave this page.");
  } catch (error) {
    if (registration === controller) message(output, error.message, "error");
  } finally {
    if (registration === controller) {
      registration = null;
      setPending(registerForm, false);
    }
  }
});

reportsForm.addEventListener("submit", (event) => {
  event.preventDefault();
  if (!rotateButton.disabled) loadReport();
});
node("load-sample").addEventListener("click", () => {
  clearReport();
  renderReport(sampleReport(Number(periodInput.value)), "Paperplane Assistant", "sample");
  message(status, "Showing sample data. No server was contacted.");
});
periodInput.addEventListener("change", () => {
  if (activeReport === "sample") node("load-sample").click();
  else if (activeReport === "live" || request) loadReport();
});
tokenInput.addEventListener("input", () => {
  if (activeReport === "sample") return;
  clearReport();
  message(status, "");
});
apiInput.addEventListener("change", () => {
  registration?.abort();
  registration = null;
  setPending(registerForm, false);
  created = null;
  showCreatedCredentials();
  tokenInput.value = "";
  clearReport();
  message(node("creation-error"), "");
  message(status, "");
  try {
    const origin = serverOrigin();
    node("hosted-storage-note").hidden = !hostedServers.has(origin);
    message(node("api-url-status"), `Using ${origin}`);
  } catch (error) {
    node("hosted-storage-note").hidden = true;
    message(node("api-url-status"), error.message, "error");
  }
});

node("show-token").addEventListener("click", (event) => {
  showCreatedCredentials(event.currentTarget.getAttribute("aria-pressed") !== "true");
});

async function copy(value, label) {
  if (!value) return;
  try {
    await navigator.clipboard.writeText(value);
    message(node("creation-error") ?? status, `${label} copied.`);
  } catch {
    message(node("creation-error") ?? status, `Could not access the clipboard. Select the ${label.toLowerCase()} and copy it manually.`, "error");
  }
}
node("copy-id").addEventListener("click", () => copy(created?.app_id, "App ID"));
node("copy-token").addEventListener("click", () => copy(created?.app_secret, "App token"));

rotateButton.addEventListener("click", () => {
  if (!activeApp || registration || rotateButton.disabled) return;
  node("rotate-target").textContent = `App “${activeApp.name}” on ${serverOrigin()}`;
  rotateDialog.returnValue = "";
  rotateDialog.showModal();
});

rotateDialog.addEventListener("close", async () => {
  if (rotateDialog.returnValue !== "rotate") return;
  if (!activeApp || registration || rotateButton.disabled) return;
  const token = tokenInput.value.trim();
  const app = activeApp;
  const origin = serverOrigin();
  if (!isToken(token)) return message(status, "Paste the current app token first.", "error");
  const controls = [apiInput, tokenInput, periodInput, rotateButton, node("load-sample")];
  controls.forEach((control) => { control.disabled = true; });
  setPending(registerForm, true);
  setPending(reportsForm, true);
  message(status, "Rotating app token…");
  try {
    const data = await call(origin, "/v1/app/secret", { method: "POST", headers: { Authorization: `Bearer ${token}` } });
    if (!isToken(data.app_secret)) throw new Error("The server did not return a usable replacement token.");
    created = { app_id: app.app_id, app_secret: data.app_secret };
    tokenInput.value = data.app_secret;
    showCreatedCredentials();
    message(status, "Token rotated. Update your services with the new token.");
    message(node("creation-error"), "Replacement token created. Save it before you leave this page.");
    node("copy-token").focus();
  } catch (error) {
    message(status, error.message, "error");
  } finally {
    controls.forEach((control) => { control.disabled = false; });
    setPending(registerForm, false);
    setPending(reportsForm, false);
  }
});
