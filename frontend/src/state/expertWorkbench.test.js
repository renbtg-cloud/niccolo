import { describe, it, expect } from 'vitest';
import { assessmentRequest, assessmentResults, assessmentKey } from './expertWorkbench.js';

describe('contextual assessment request', () => {
  const draft = { dimension: 'RELIABILITY', result: 'RELIABLE', confidence: 'LOW', criterion: ' consistency ', context: ' current investigation ', rationale: ' because of provenance ' };
  const target = {kind: 'OBJECT', object_id: 'object'};
  const ground = {kind: 'EVIDENCE', evidence_id: 'evidence'};
  it('retains typed context, grounds and revision without numeric aggregation', () => {
    expect(assessmentRequest({...draft, revises_event_id: 'prior'}, target, [ground])).toEqual({ ...draft, criterion: 'consistency', context: 'current investigation', rationale: 'because of provenance', target, grounds: [ground], revises_event_id: 'prior' });
    expect(assessmentKey({kind:'ROUTE', argument_id:'a', grounding_set_id:'g'})).toBe('ROUTE:a:g');
  });
  it('prevents cross-dimensional results and duplicate grounds', () => {
    expect(Object.keys(assessmentResults)).toHaveLength(5);
    expect(() => assessmentRequest({...draft, result:'CREDIBLE'}, target, [])).toThrow();
    expect(() => assessmentRequest(draft, target, [ground, ground])).toThrow();
  });
});
