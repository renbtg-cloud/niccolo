export function reanalysisChanges(view) {
  if (!view) return [];
  const rows = [
    ['Focus changed', view.focus_changed ? 1 : 0],
    ['Sources added', view.added_source_count ?? 0],
    ['Sources removed', view.removed_source_count ?? 0],
    ['Human context added', view.added_context_count ?? 0],
    ['Human context removed', view.removed_context_count ?? 0],
    ['Claims retained', view.retained ?? 0],
    ['Claims refined', view.refined ?? 0],
    ['Claims reversed', view.reversed ?? 0],
    ['Claims dropped', view.dropped ?? 0],
    ['Claims split', view.split ?? 0],
    ['Claims merged', view.merged ?? 0],
  ];
  return rows.filter(([,count]) => count > 0).map(([label,count]) => ({ label, count }));
}
