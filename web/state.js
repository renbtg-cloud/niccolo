export function initialState() {
  return Object.freeze({
    intention: null,
    focus: "",
    premises: Object.freeze([]),
    sources: Object.freeze([]),
    revision: 1,
    history: Object.freeze([{ revision: 1, reason: "Situation opened" }]),
    nextPremiseId: 1,
  });
}

function checkpoint(state, patch, reason) {
  const revision = state.revision + 1;
  return Object.freeze({
    ...state,
    ...patch,
    revision,
    history: Object.freeze([...state.history, { revision, reason }]),
  });
}

export function reduce(state, action) {
  switch (action.type) {
    case "select-intention":
      return checkpoint(state, { intention: action.value }, "Intention changed");
    case "set-focus": {
      const value = action.value.trim();
      if (!value) return state;
      return checkpoint(state, { focus: value }, "Focus changed");
    }
    case "add-source": {
      const name = action.name.trim();
      if (!name || state.sources.includes(name)) return state;
      return checkpoint(
        state,
        { sources: Object.freeze([...state.sources, name]) },
        `Source catalogued: ${name}`,
      );
    }
    case "add-premise": {
      const text = action.text.trim();
      if (!text) return state;
      const premise = Object.freeze({
        id: state.nextPremiseId,
        text,
        kind: action.hypothetical ? "hypothetical" : "user-reported",
        active: true,
      });
      return checkpoint(
        state,
        {
          premises: Object.freeze([...state.premises, premise]),
          nextPremiseId: state.nextPremiseId + 1,
        },
        action.hypothetical ? "WHAT IF added" : "User context added",
      );
    }
    case "retract-premise": {
      let changed = false;
      const premises = state.premises.map((p) => {
        if (p.id !== action.id || !p.active) return p;
        changed = true;
        return Object.freeze({ ...p, active: false });
      });
      if (!changed) return state;
      return checkpoint(
        state,
        { premises: Object.freeze(premises) },
        `Premise ${action.id} retracted`,
      );
    }
    default:
      return state;
  }
}
