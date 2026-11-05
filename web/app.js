import { initialState, reduce } from "/state.js";

let state = initialState();
const $ = (id) => document.getElementById(id);

function render() {
  const selected = $("selected-intent");
  const labels = {
    act: "I am about to say or do something",
    understand: "I am trying to understand something",
    discover: "Help me figure out what matters",
  };
  selected.textContent = state.intention ? `Current intention: ${labels[state.intention]}` : "No intention selected yet.";
  $("revision-count").textContent = `revision ${state.revision}`;

  const sourceList = $("source-list");
  sourceList.replaceChildren(...state.sources.map((name) => {
    const li = document.createElement("li");
    li.textContent = `${name} — catalogued, not analyzed`;
    return li;
  }));

  const premiseList = $("premise-list");
  premiseList.replaceChildren(...state.premises.map((premise) => {
    const li = document.createElement("li");
    const text = document.createElement("span");
    text.textContent = `${premise.kind === "hypothetical" ? "WHAT IF" : "USER-REPORTED"}: ${premise.text}`;
    if (!premise.active) text.className = "retracted";
    li.append(text);
    if (premise.active) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "text-button";
      button.textContent = "Retract";
      button.addEventListener("click", () => {
        state = reduce(state, { type: "retract-premise", id: premise.id });
        render();
      });
      li.append(button);
    }
    return li;
  }));

  const revisions = $("revision-list");
  revisions.replaceChildren(...state.history.map((entry) => {
    const li = document.createElement("li");
    li.textContent = `r${entry.revision}: ${entry.reason}`;
    return li;
  }));
}

$("intentions").addEventListener("click", (event) => {
  const button = event.target.closest("button[data-intent]");
  if (!button) return;
  state = reduce(state, { type: "select-intention", value: button.dataset.intent });
  render();
});

$("sources").addEventListener("change", (event) => {
  for (const file of event.target.files ?? []) {
    state = reduce(state, { type: "add-source", name: file.name });
  }
  render();
});

$("set-focus").addEventListener("click", () => {
  state = reduce(state, { type: "set-focus", value: $("focus").value });
  render();
});

$("add-premise").addEventListener("click", () => {
  const field = $("premise");
  state = reduce(state, {
    type: "add-premise",
    text: field.value,
    hypothetical: $("what-if").checked,
  });
  field.value = "";
  $("what-if").checked = false;
  render();
});

render();
