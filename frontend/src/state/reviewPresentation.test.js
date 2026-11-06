import { describe, it, expect } from 'vitest';
import { coverageSignals, recordedChanges, proactiveLeads, reanalysisPrompt } from './reviewPresentation.js';

describe('review presentation', () => {
  it('presents recorded coverage without converting counts into completeness', () => {
    expect(coverageSignals({ examined_fragment_count: 3, catalogued_fragment_count: 10, used_context_count: 2, input_context_count: 4 })).toEqual([
      { label: 'Source fragments examined', used: 3, total: 10 },
      { label: 'Human context used', used: 2, total: 4 },
    ]);
  });

  it('shows only explicit post-analysis Situation changes', () => {
    expect(recordedChanges({ input_is_current: false, focus_changed: true, added_source_count: 2, removed_source_count: 0, added_context_count: 1, removed_context_count: 1 })).toEqual([
      'Focus changed', '2 source(s) added', '1 context contribution(s) added', '1 context contribution(s) removed',
    ]);
    expect(recoredChanges({ input_is_current: true, focus_changed: true, added_source_count: 9 })).toEqual([]);
  });

  it('preserves backend boundaries for proactive leads and reanalysis', () => {
    const dashboard = {
      proactive: { leads: [{ id: 'l', kind: 'EVIDENCE_GAP', text: 'Check the original message', why_now: 'missing support', user_may_ignore: true, lead_is_evidence: false, lead_establishes_truth: false }] },
      reanalysis: { recommended: true, reasons: ['1 source(s) added'], human_approval_required: true, recommendation_starts_analysis: false, old_output_must_be_preserved: true },
    };
    expect(proactiveLeads(dashboard)[0]).toMatchObject({ userMayIgnore: true, isEvidence: false, establishesTruth: false });
    expect(reanalysisPrompt(dashboard)).toEqual({ reasons: ['1 source(s) added'], humanApprovalRequired: true, startsAutomatically: false, preservesOldOutput: true });
  });
});
