import { describe, it, expect } from 'vitest';
import { assessmentRequest, assessmentResults, assessmentKey, assessmentDependencyRequest, assessmentDependencyReviewRequest, assessmentDownstreamReviewRequest, assessmentTargetRevision, nodeRequest, defaultGroundRole, relationEndpointAllowed, assessmentDimensionsForTarget, integrityDimensions, objectionAttentionGroups } from './expertWorkbench.js';

describe('contextual assessment request', () => {
  const draft = { dimension: 'RELIABILITY', result: 'RELIABLE', confidence: 'LOW', criterion: ' consistency ', context: ' current investigation ', rationale: ' because of provenance ' };
  const target = {kind: 'OBJECT', object_id: 'object'};
  const ground = {kind: 'EVIDENCE', evidence_id: 'evidence'};
  it('retains typed context, grounds and revision without numeric aggregation', () => {
    expect(assessmentRequest({...draft, revises_event_id: 'prior'}, target, [ground])).toEqual({ ...draft, criterion: 'consistency', context: 'current investigation', rationale: 'because of provenance', target, grounds: [ground], revises_event_id: 'prior' });
    expect(assessmentKey({kind:'ROUTE', argument_id:'a', grounding_set_id:'g'})).toBe('ROUTE:a:g');
  });
  it('prevents cross-dimensional results and duplicate grounds', () => {
    expect(Object.keys(assessmentResults)).toHaveLength(33);
    expect(() => assessmentRequest({...draft, result:'CREDIBLE'}, target, [])).toThrow();
    expect(() => assessmentRequest(draft, target, [ground, ground])).toThrow();
  });
  it('records objection triage dimensions independently and never confuses review priority with truth', () => {
    const route = {kind:'ROUTE',argument_id:'a',grounding_set_id:'g'};
    const results = { OBJECTION_NOVELTY:'DUPLICATIVE', OBJECTION_MATERIALITY:'LIMITED_MATERIALITY', OBJECTION_REVIEW_COST:'HIGH_COST', OBJECTION_ATTENTION_DISPOSITION:'DEFER_REVIEW' };
    for (const [dimension,result] of Object.entries(results)) {
      expect(assessmentDimensionsForTarget(route,[])).toContain(dimension);
      expect(assessmentRequest({...draft,dimension,result},route,[ground])).toMatchObject({target:route,dimension,result});
      expect(() => assessmentRequest({...draft,dimension,result:'CREDIBLE'},route,[])).toThrow();
    }
  });
  it('groups only explicitly triaged targets with distinct current disagreements and preserved revision counts', () => {
    const route = {kind:'ROUTE',argument_id:'a',grounding_set_id:'g'};
    const other = {kind:'ROUTE',argument_id:'a',grounding_set_id:'other'};
    const record = (event_id,dimension,result,target=route,revises_event_id=null) => ({event_id,input:{...draft,dimension,result,target,revises_event_id}});
    const history = [record('a','OBJECTION_NOVELTY','DUPLICATIVE'),record('b','OBJECTION_NOVELTY','PARTIALLY_NOVEL',route,'a'),record('c','OBJECTION_ATTENTION_DISPOSITION','DEFER_REVIEW'),record('d','OBJECTION_ATTENTION_DISPOSITION','REVIEW_NOW'),record('e','RELEVANCE','LIMITED_RELEVANCE'),record('f','RELEVANCE','RELEVANT',other)];
    expect(objectionAttentionGroups(history)).toEqual([{key:'ROUTE:a:g',target:route,historicalCount:1,current:history.slice(1,5)}]);
    expect(objectionAttentionGroups([history[5]])).toEqual([]);
  });
  it('separates recommendation suitability from three historical decision perspectives', () => {
    const recommendation = {kind:'RECOMMENDATION',recommendation_id:'r1'};
    const decision = {kind:'DECISION',decision_id:'d1'};
    const dimensions = ['DECISION_CONTEMPORANEOUS_REASONABLENESS','DECISION_ACTUAL_CIRCUMSTANCES_ADEQUACY','DECISION_REASONABLY_OBTAINABLE_INFORMATION'];
    expect(assessmentDimensionsForTarget(recommendation,[])).toEqual(['RECOMMENDATION_APPROPRIATENESS']);
    expect(assessmentDimensionsForTarget(decision,[])).toEqual(dimensions);
    expect(assessmentKey(recommendation)).toBe('RECOMMENDATION:r1');
    expect(assessmentKey(decision)).toBe('DECISION:d1');
    for (const dimension of dimensions) {
      const judgment = {...draft,dimension,result:'INADEQUATE'};
      expect(assessmentRequest(judgment,decision,[ground])).toMatchObject({target:decision,dimension,result:'INADEQUATE'});
      expect(() => assessmentRequest(judgment,recommendation,[])).toThrow('exact recommendation or decision');
      expect(() => assessmentRequest(judgment,target,[])).toThrow();
    }
    expect(() => assessmentRequest(draft,decision,[])).toThrow();
    expect(assessmentRequest({...draft,dimension:'RECOMMENDATION_APPROPRIATENESS',result:'CONTESTED'},recommendation,[]).target).toEqual(recommendation);
  });
  it('shows explicit action successors without changing the historical assessment target', () => {
    const recommendation = {kind:'RECOMMENDATION',recommendation_id:'r1'};
    const decision = {kind:'DECISION',decision_id:'d1'};
    const recommendations = [{recommendation_id:'r2',input:{revises_recommendation_id:'r1'}}];
    const decisions = [{decision_id:'d2',input:{revises_decision_id:'d1'}},{decision_id:'d3',input:{revises_decision_id:'d2'}}];
    expect(assessmentTargetRevision(recommendation,[],recommendations,decisions)).toBe('r2');
    expect(assessmentTargetRevision(decision,[],recommendations,decisions)).toBe('d3');
    expect(assessmentRequest({...draft,dimension:'DECISION_CONTEMPORANEOUS_REASONABLENESS',result:'ADEQUATE'},decision,[]).target).toEqual(decision);
  });
  it('keeps question selection criteria separate and restricts them to questions', () => {
    const q = {kind:'OBJECT',object_id:'q'};
    const nodes = [{object_id:'q',kind:'QUESTION'},{object_id:'object',kind:'PROPOSITION'}];
    const authorization = {...draft,dimension:'AUTHORIZATION_STATUS',result:'AUTHORIZATION_REPORTED'};
    expect(assessmentDimensionsForTarget(q,nodes)).toContain('CONTAMINATION_RISK');
    expect(assessmentDimensionsForTarget(target,nodes)).not.toContain('CONTAMINATION_RISK');
    expect(assessmentRequest(authorization,q,[],nodes).result).toBe('AUTHORIZATION_REPORTED');
    expect(() => assessmentRequest(authorization,target,[],nodes)).toThrow('investigative question');
    expect(() => assessmentRequest({...authorization,result:'LOW_COST'},q,[],nodes)).toThrow();
  });
  it('keeps ten investigation integrity aspects independently contestable without a score or veto', () => {
    expect(integrityDimensions).toHaveLength(10);
    for (const dimension of integrityDimensions) {
      const item = {...draft, dimension, result:'CONCERN_INDICATED'};
      expect(assessmentRequest(item,target,[ground])).toMatchObject({dimension,result:'CONCERN_INDICATED',target,grounds:[ground]});
      expect(assessmentRequest({...item,result:'CONTESTED'},target,[]).result).toBe('CONTESTED');
      expect(()=>assessmentRequest({...item,result:'UNRELIABLE'},target,[])).toThrow();
    }
    expect(()=>assessmentRequest({...draft,result:'COMPROMISE_INDICATED'},target,[])).toThrow();
    expect(assessmentDimensionsForTarget({kind:'ASSESSMENT',assessment_event_id:'prior'},[])).toContain('INTEGRITY_REVIEWABILITY');
  });
  it('keeps compromise, containment and recovery separate without authorizing a change',()=>{
    for(const [dimension,result] of [['MATERIAL_COMPROMISE','COMPROMISE_INDICATED'],['RELIANCE_CONTAINMENT','PAUSE_INDICATED'],['RECOVERY_READINESS','RECOVERY_NOT_READY']]){
      expect(assessmentRequest({...draft,dimension,result},target,[ground])).toMatchObject({dimension,result,target,grounds:[ground]});
      expect(assessmentRequest({...draft,dimension,result:'UNKNOWN'},target,[]).result).toBe('UNKNOWN');
    }
    expect(()=>assessmentRequest({...draft,dimension:'RECOVERY_READINESS',result:'NO_PAUSE_INDICATED'},target,[])).toThrow();
    expect(()=>assessmentRequest({...draft,dimension:'MATERIAL_COMPROMISE',result:'RECOVERY_READY'},target,[])).toThrow();
  });
  it('restricts gap resolution to gaps and requires non-question grounds for a supported or partially addressed result', () => {
    const nodes = [{object_id:'g',kind:'INFORMATION_GAP'},{object_id:'q',kind:'QUESTION'},{object_id:'p',kind:'PROPOSITION'}];
    const gap = {kind:'OBJECT',object_id:'g'};
    const q = {kind:'OBJECT',object_id:'q'};
    const p = {kind:'OBJECT',object_id:'p'};
    const resolution = {...draft,dimension:'GAP_RESOLUTION',result:'SUPPORTED'};
    expect(assessmentDimensionsForTarget(gap,nodes)).toContain('GAP_RESOLUTION');
    expect(assessmentDimensionsForTarget(q,nodes)).not.toContain('GAP_RESOLUTION');
    expect(() => assessmentRequest(resolution,q,[p],nodes)).toThrow('information gap');
    expect(() => assessmentRequest(resolution,gap,[q,gap],nodes)).toThrow('explicit ground');
    expect(assessmentRequest(resolution,gap,[p],nodes).result).toBe('SUPPORTED');
    expect(assessmentRequest({...resolution,result:'UNRESOLVED'},gap,[],nodes).grounds).toEqual([]);
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
  it('exposes explicit target successors without retargeting meta-assessments', () => {
    const target = {kind:'ASSESSMENT', assessment_event_id:'original'};
    const events = [
      {event_id:'original', input:{target:{kind:'OBJECT',object_id:'object'}}},
      {event_id:'competing', input:{target:{kind:'OBJECT',object_id:'object'}}},
      {event_id:'revision', input:{revises_event_id:'original'}},
      {event_id:'latest', input:{revises_event_id:'revision'}},
    ];
    expect(assessmentTargetRevision(target,events)).toBe('latest');
    expect(assessmentTargetRevision(target,events.slice(0,2))).toBeNull();
    expect(assessmentTargetRevision({kind:'OBJECT',object_id:'object'},events)).toBeNull();
    expect(assessmentRequest(draft,target,[]).target).toEqual(target);
    expect(assessmentTargetRevision(target,[...events,{event_id:'original', input:{revises_event_id:'latest'}}])).toBeNull();
  });
});

describe('investigative question semantics', () => {
  it('allows a question as a distinct authored node and context, not a premise', () => {
    expect(nodeRequest('QUESTION','Why did the manager lie?', 'Investigate an allegation').kind).toBe('QUESTION');
    expect(defaultGroundRole('QUESTION')).toBe('CONTEXT');
    expect(relationEndpointAllowed('SEMANTIC','ENTAILS','QUESTION','HYPOTHESIS')).toBe(false);
    expect(relationEndpointAllowed('CONTEXTUAL','RELEVANT_TO','INFORMATION_GAP','QUESTION')).toBe(true);
    expect(relationEndpointAllowed('GENEALOGY','REFINES','QUESTION','QUESTION')).toBe(true);
    expect(relationEndpointAllowed('GENEALOGY','REFINES','QUESTION','HYPOTHESIS')).toBe(false);
  });
  it('restricts directed investigative links to their semantic endpoint types', () => {
    expect(relationEndpointAllowed('CONTEXTUAL','ADDRESSES_GAP','QUESTION','INFORMATION_GAP')).toBe(true);
    expect(relationEndpointAllowed('CONTEXTUAL','ADDRESSES_GAP','INFORMATION_GAP','QUESTION')).toBe(false);
    expect(relationEndpointAllowed('CONTEXTUAL','TESTS_HYPOTHESIS','HYPOTHESIS','QUESTION','right')).toBe(true);
    expect(relationEndpointAllowed('CONTEXTUAL','TESTS_HYPOTHESIS','FINDING','QUESTION','right')).toBe(false);
    expect(relationEndpointAllowed('CONTEXTUAL','PRESUPPOSES','ASSUMPTION','QUESTION','right')).toBe(true);
    expect(relationEndpointAllowed('CONTEXTUAL','PRESUPPOSES','INFORMATION_GAP','QUESTION','right')).toBe(false);
  });
});
