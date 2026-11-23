import { describe, it, expect } from 'vitest';
import { coverageSignals, recordedChanges, proactiveLeads, reanalysisPrompt } from './reviewPresentation.js';

describe('review presentation', () => {
  it('presents recorded coverage without converting counts into completeness', () => {
    expect(coverageSignals({ sources: { examined_fragments: 3, catalogued_fragments: 10 }, human_context: { recorded_as_used: 2, present_in_input: 4 } })).toEqual([
      { label: 'Source fragments examined', used: 3, total: 10 },
      { label: 'Human context used', used: 2, total: 4 },
    ]);
  });

  it('shows only explicit post-analysis Situation changes', () => {
    expect(recordedChanges({ input_is_current: false, delta: { focus_changed: true, sources_added: 2, sources_removed: 0, context_added: 1, context_removed: 1 } })).toEqual([
      'Focus changed', '2 source(s) added', '1 context contribution(s) added', '1 context contribution(s) removed',
    ]);
    expect(recordedChanges({ input_is_current: true, delta: { focus_changed: true, sources_added: 9 } })).toEqual([]);
  });

  it('preserves backend boundaries for proactive leads and reanalysis', () => {
    const dashboard = {
      proactive: { leads: [{ id: 'l', kind: 'EVIDENCE_GAP', text: 'Check the original message', why_now: 'missing support', user_may_ignore: true, lead_is_evidence: false, lead_establishes_truth: false }] },
      reanalysis: { recommended: true, reasons: ['1 source(s) added'], human_approval_required: true, recommendation_starts_analysis: false, old_output_must_be_preserved: true },
    };
    expect(proactiveLeads(dashboard)[0]).toMatchObject({ label: 'Evidence gap', userMayIgnore: true, isEvidence: false, establishesTruth: false });
    expect(reanalysisPrompt(dashboard)).toEqual({ reasons: ['1 source(s) added'], humanApprovalRequired: true, startsAutomatically: false, preservesOldOutput: true });
  });
});
