import { describe, it, expect } from 'vitest';
import { assessmentRequest, assessmentResults, assessmentKey, assessmentDependencyRequest, assessmentDependencyReviewRequest, assessmentDownstreamReviewRequest } from './expertWorkbench.js';

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
  it('pins an exact event and binds a review to the current revision', () => {
    expect(assessmentDependencyRequest({argument_id:'a', grounding_set_id:'g'}, 'old', ' reliance ')).toEqual({argument_id:'a', grounding_set_id:'g', assessment_event_id:'old', rationale:'reliance'});
    expect(assessmentDependencyReviewRequest({assessment_revised:true, current_assessment_event_id:'new'}, 'RETAINED', ' still applicable ')).toEqual({reviewed_assessment_event_id:'new', disposition:'RETAINED', note:'still applicable'});
    expect(() => assessmentDependencyReviewRequest({assessment_revised:false}, 'RETAINED','note')).toThrow();
  });
  it('binds downstream review to the exact reachable route and assessment revision', () => {
    const route={argument_id:'a',grounding_set_id:'g'};
    const view={assessment_revised:true,current_assessment_event_id:'revision',potential_downstream_routes:[route]};
    expect(assessmentDownstreamReviewRequest(view,route,'DEFERRED',' wait ')).toEqual({...route,reviewed_assessment_event_id:'revision',disposition:'DEFERRED',note:'wait'});
    expect(() => assessmentDownstreamReviewRequest(view,{argument_id:'a',grounding_set_id:'other'},'RETAINED','note')).toThrow();
  });
  it('identifies an assessment target by its exact event rather than its underlying object', () => {
    const meta={kind:'ASSESSMENT',assessment_event_id:'original'};
    expect(assessmentKey(meta)).toBe('ASSESSMENT:original');
    expect(assessmentRequest(draft,meta,[]).target).toEqual(meta);
  });
});
