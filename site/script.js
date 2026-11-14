const requestPayload = {
  model: "gpt-4.1-mini",
  messages: [
    { role: "system", content: "You are a concise assistant." },
    { role: "user", content: "Summarize these meeting notes and list the next steps: ..." }
  ],
  max_tokens: 300
};

const scopeNames = {
  once: "This request only",
  count: "Next 5 calls",
  budget: "Up to $2.00",
  trust: "Trust this app"
};

const fullData = document.querySelector("#full-data");
const inspectButton = document.querySelector("#inspect-toggle");
const responsePanel = document.querySelector("#response-panel");
const responseStatus = document.querySelector("#response-status");
const feedback = document.querySelector("#demo-feedback");
const feedbackCopy = document.querySelector("#feedback-copy");
const approveButton = document.querySelector("#approve-button");
const denyButton = document.querySelector("#deny-button");
const resetButton = document.querySelector("#reset-button");
const scopeSummary = document.querySelector("#scope-summary");
const scopeInputs = [...document.querySelectorAll('input[name="scope"]')];

let decision = null;

function selectedScope() {
  return scopeInputs.find((input) => input.checked)?.value ?? "once";
}

function renderInspection() {
  const simulatedResponse = decision === "approved"
    ? {
        id: "sim_chatcmpl_masterkey_demo",
        object: "chat.completion",
        model: requestPayload.model,
        choices: [{ index: 0, message: { role: "assistant", content: "The notes cover the project update. Next steps: confirm owners, share the revised timeline, and schedule a follow-up." }, finish_reason: "stop" }],
        usage: { prompt_tokens: 34, completion_tokens: 27, total_tokens: 61 },
        _demo: "Simulated response only. No provider was contacted."
      }
    : decision === "denied"
      ? { error: { type: "masterkey_denied", message: "The simulated user denied this request." }, _demo: "Simulated response only. No server was contacted." }
      : null;

  fullData.textContent = JSON.stringify({ request: requestPayload, simulated_response: simulatedResponse }, null, 2);
  responseStatus.textContent = decision ? "SIMULATED" : "SIMULATION DATA";
}

function updateScope() {
  const scope = selectedScope();
  scopeSummary.textContent = scopeNames[scope];
  document.querySelectorAll(".scope-option").forEach((label) => {
    label.classList.toggle("selected", label.querySelector("input").checked);
  });
}

function finish(nextDecision) {
  if (decision !== null) return;
  decision = nextDecision;
  const approved = nextDecision === "approved";
  feedback.dataset.state = nextDecision;
  feedback.querySelector(".feedback-icon").textContent = approved ? "✓" : "×";
  feedbackCopy.textContent = approved
    ? `Simulated approval: ${scopeNames[selectedScope()]}. A sample response is available below. No request was sent.`
    : "Simulated denial. A sample error is available below. No request was sent.";
  approveButton.hidden = true;
  denyButton.hidden = true;
  resetButton.hidden = false;
  renderInspection();
  responsePanel.hidden = false;
  inspectButton.setAttribute("aria-expanded", "true");
  inspectButton.innerHTML = 'Hide the full payload and response <span aria-hidden="true">−</span>';
  document.querySelectorAll('input[name="scope"]').forEach((input) => { input.disabled = true; });
}

function resetDemo() {
  decision = null;
  feedback.dataset.state = "";
  feedback.querySelector(".feedback-icon").textContent = "i";
  feedbackCopy.textContent = "Review the payload, choose an access scope, then make your call.";
  approveButton.hidden = false;
  denyButton.hidden = false;
  resetButton.hidden = true;
  responsePanel.hidden = true;
  inspectButton.setAttribute("aria-expanded", "false");
  inspectButton.innerHTML = 'Inspect the full payload <span aria-hidden="true">＋</span>';
  document.querySelectorAll('input[name="scope"]').forEach((input) => { input.disabled = false; });
  document.querySelector('input[name="scope"][value="once"]').checked = true;
  updateScope();
  renderInspection();
}

inspectButton.addEventListener("click", () => {
  const opening = inspectButton.getAttribute("aria-expanded") !== "true";
  responsePanel.hidden = !opening;
  inspectButton.setAttribute("aria-expanded", String(opening));
  inspectButton.innerHTML = opening
    ? 'Hide the full payload and response <span aria-hidden="true">−</span>'
    : 'Inspect the full payload <span aria-hidden="true">＋</span>';
});
scopeInputs.forEach((input) => input.addEventListener("change", updateScope));
approveButton.addEventListener("click", () => finish("approved"));
denyButton.addEventListener("click", () => finish("denied"));
resetButton.addEventListener("click", resetDemo);

const tabs = [...document.querySelectorAll(".code-tab")];
function activateTab(tab) {
  tabs.forEach((item) => {
    const active = item === tab;
    item.classList.toggle("active", active);
    item.setAttribute("aria-selected", String(active));
    item.tabIndex = active ? 0 : -1;
    document.querySelector(`#code-${item.dataset.tab}`).hidden = !active;
  });
}
tabs.forEach((tab, index) => {
  tab.addEventListener("click", () => activateTab(tab));
  tab.addEventListener("keydown", (event) => {
    let nextIndex = index;
    if (event.key === "ArrowRight") nextIndex = (index + 1) % tabs.length;
    else if (event.key === "ArrowLeft") nextIndex = (index - 1 + tabs.length) % tabs.length;
    else if (event.key === "Home") nextIndex = 0;
    else if (event.key === "End") nextIndex = tabs.length - 1;
    else return;
    event.preventDefault();
    activateTab(tabs[nextIndex]);
    tabs[nextIndex].focus();
  });
});

updateScope();
renderInspection();
