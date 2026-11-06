export function historyGroups(entries = []) {
  const byId = new Map(entries.map((entry) => [entry.analysis_id, entry]));
  const roots = [];
  const children = new Map();
  for (const entry of entries) {
    if (entry.reanalysis_of && byId.has(entry.reanalysis_of)) {
      const list = children.get(entry.reanalysis_of) ?? [];
      list.push(entry);
      children.set(entry.reanalysis_of, list);
    } else {
      roots.push(entry);
    }
  }
  const rows = [];
  const visit = (entry, depth) => {
    rows.push({ ...entry, depth });
    for (const child of children.get(entry.analysis_id) ?? []) visit(child, depth + 1);
  };
  for (const root of roots) visit(root, 0);
  return rows;
}

export function analysisHistoryLabel(entry) {
  if (!entry.answer_text) return 'No published answer';
  return entry.input_is_current ? 'Current input' : 'Historical input preserved';
}
