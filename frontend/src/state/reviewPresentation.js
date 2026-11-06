export function coverageSignals(review) {
  if (!review) return [];
  return [
    { label: 'Source fragments examined', used: review.examined_fragment_count ?? 0, total: review.catalogued_fragment_count ?? 0 },
    { label: 'Human context used', used: review.used_context_count ?? 0, total: review.input_context_count ?? 0 },
  ];
}

export function recordedChanges(review) {
  if (!review || review.input_is_current) return [];
  const items = [];
  if (review.focus_changed) items.push('Focus changed');
  if (review.added_source_count) items.push(`${review.added_source_count} source(s) added`);
  if (review.removed_source_count) items.push(`${review.removed_source_count} source(s) removed`);
  if (review.added_context_count) items.push(`${review.added_context_count} context contribution(s) added`);
  if (review.removed_context_count) items.push(`${review.removed_context_count} context contribution(s) removed`);
  return items;
}

export function proactiveLeads(dashboard) {
  return (dashboard?.proactive?.leads ?? []).map((lead) => ({
    id: lead.id,
    kind: lead.kind,
    text: lead.text,
    whyNow: lead.why_now,
    userMayIgnore: lead.user_may_ignore === true,
    isEvidence: lead.lead_is_evidence === true,
    establishesTruth: lead.lead_establishes_truth === true,
  }));
}

export function reanalysisPrompt(dashboard) {
  const r = dashboard?.reanalysis;
  if (!r?.recommended) return null;
  return {
    reasons: r.reasons ?? [],
    humanApprovalRequired: r.human_approval_required === true,
    startsAutomatically: r.recommendation_starts_analysis === true,
    preservesOldOutput: r.old_output_must_be_preserved === true,
  };
}
