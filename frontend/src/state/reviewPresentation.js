export function coverageSignals(review) {
  if (!review) return [];
  return [
    { label: 'Source fragments examined', used: review.sources?.examined_fragments ?? 0, total: review.sources?.catalogued_fragments ?? 0 },
    { label: 'Human context used', used: review.human_context?.recorded_as_used ?? 0, total: review.human_context?.present_in_input ?? 0 },
  ];
}

export function recordedChanges(review) {
  if (!review || review.input_is_current) return [];
  const { delta } = review;
  if (!delta) return [];
  const items = [];
  if (delta.focus_changed) items.push('Focus changed');
  if (delta.sources_added) items.push(`${delta.sources_added} source(s) added`);
  if (delta.sources_removed) items.push(`${delta.sources_removed} source(s) removed`);
  if (delta.context_added) items.push(`${delta.context_added} context contribution(s) added`);
  if (delta.context_removed) items.push(`${delta.context_removed} context contribution(s) removed`);
  return items;
}

const leadLabels = {
  EVIDENCE_GAP: 'Evidence gap',
  CONFLICT: 'Conflicting evidence',
  SOURCE_REQUEST: 'Source to look for',
  CLARIFICATION: 'Question to clarify',
};

export function proactiveLeads(dashboard) {
  return (dashboard?.proactive?.leads ?? []).map((lead) => ({
    id: lead.id,
    kind: lead.kind,
    label: leadLabels[lead.kind] ?? 'Investigation lead',
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
