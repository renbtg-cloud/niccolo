import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { NiccoloClient } from '../api/client.js';
import { applicabilityRequest, groundingRequest, nodeRequest, relationRequest, relationSemantics, reassessmentReviewRequest, questionResponseRequest, interpretationDependencyRequest, interpretationReviewRequest } from '../state/expertWorkbench.js';

const app = readFileSync(new URL('../App.jsx', import.meta.url), 'utf8');
const page = readFileSync(new URL('./ExpertWorkbenchPage.jsx', import.meta.url), 'utf8');
const meaningPanel = readFileSync(new URL('./ResponseInterpretationPanel.jsx', import.meta.url), 'utf8');
const comparisonPanel = readFileSync(new URL('./InquiryComparisonPanel.jsx', import.meta.url), 'utf8');
const config = {
  workspace: '11111111-1111-4111-8111-111111111111',
  situation: '22222222-2222-4222-8222-222222222222',
  token: 'test-in-memory-credential',
};
const response = { status: 200, ok: true, headers: { get: () => null }, json: async () => ({ nodes: [], evidence: [], evidence_truncated: false, relations: [], arguments: [], grounding_sets: [], reassessment_impacts: [], grounding_cycles: [], reasoning_diagnostics: [] }) };

describe('expert epistemic Workbench — structured MVP authoring', () => {
  it('authors exact historical action assessments without treating hindsight as contemporaneous knowledge', () => {
    expect(page).toContain("kind:'RECOMMENDATION',recommendation_id:r.recommendation_id");
    expect(page).toContain("kind:'DECISION',decision_id:d.decision_id");
    expect(page).toContain('Do not substitute later discoveries for contemporaneous knowledge.');
    expect(page).toContain('Obtainability alone establishes neither actual knowledge nor negligence.');
    expect(page).toContain('This judgment does not assess the successor.');
    expect(page).toContain('This judgment does not authorize implementation.');
  });
  it('distinguishes an authored gap used as support from a verified absence', () => {
    expect(page).toContain('diagnostic.information_gap_supports');
    expect(page).toContain('gap.supporting_routes');
    expect(page).toContain('not by itself evidence of absence, concealment or misconduct');
    expect(page).toContain('Both contradictory grounds and a gap are used as support.');
    expect(page).toContain('no self-sealing verdict is inferred');
  });
  it('shows exact contradictory supporting routes as review candidates, never a self-sealing verdict', () => {
    expect(page).toContain('diagnostic.contradictory_supports');
    expect(page).toContain('candidate.left_supporting_routes');
    expect(page).toContain('candidate.right_supporting_routes');
    expect(page).toContain('conditional or alternative routes can be legitimate');
    expect(page).toContain('Absence of a warning does not establish falsifiability.');
    expect(page).toContain('No conclusion or route is automatically rejected.');
  });
  it('records a human inquiry decision without executing or granting authorization', async () => {
    const calls=[];
    const client=new NiccoloClient(config,async(url,options)=>{calls.push({url,options});return response;});
    const body={question_id:'q',considered_question_ids:['q'],assessment_event_ids:[],disposition:'PURSUE_NEXT',rationale:'Human choice'};
    await client.recordInquiryDecision('decision',body);
    expect(calls[0].url).toContain('/epistemic/question-decisions/decision');
    expect(JSON.parse(calls[0].options.body)).toEqual(body);
    expect(comparisonPanel).toContain('Record decision only');
    expect(comparisonPanel).toContain('does not send a question, execute an action, grant permission or close a gap');
    expect(comparisonPanel).toContain('subsequently revised; original basis retained');
  });
  it('reviews only affected interpretation routes against the exact changed meaning context', async () => {
    const view={dependency:{input:{argument_id:'a',grounding_set_id:'g'}},meaning_confirmation_changed:true,current_revision_id:'r',current_confirmation_event_id:'c',potential_downstream_routes:[{argument_id:'b',grounding_set_id:'h'}]};
    const body=interpretationReviewRequest(view,view.potential_downstream_routes[0],'RETAINED','Reviewed separately');
    expect(body).toMatchObject({argument_id:'b',grounding_set_id:'h',reviewed_revision_id:'r',reviewed_confirmation_event_id:'c'});
    expect(()=>interpretationReviewRequest(view,{argument_id:'x',grounding_set_id:'y'},'RETAINED','No')).toThrow();
    const calls=[];
    const client=new NiccoloClient(config,async(url,options)=>{calls.push({url,options});return response;});
    await client.reviewInterpretationDependency('d','event',body);
    expect(calls[0].url).toContain('/epistemic/interpretation-dependencies/d/reviews/event');
    expect(page).toContain('Retaining the directly relying route does not review downstream routes.');
    expect(page).toContain('view.downstream_reviews');
  });
  it('pins exact interpretation and confirmation context, not a mutable current meaning', async () => {
    const meaning={interpretation_id:'i',revisions:[{revision_id:'r1'},{revision_id:'r2'}],confirmations:[{revision_id:'r1',event_id:'c1'},{revision_id:'r2',event_id:'c2'}]};
    const body=interpretationDependencyRequest({argument_id:'a',grounding_set_id:'g'},meaning,'r1','Provisional reliance');
    expect(body).toMatchObject({interpretation_id:'i',revision_id:'r1',confirmation_event_id:'c1'});
    const calls=[];
    const client=new NiccoloClient(config,async(url,options)=>{calls.push({url,options});return response;});
    await client.recordInterpretationDependency('d',body);
    expect(calls[0].url).toContain('/epistemic/interpretation-dependencies/d');
    expect(page).toContain('Meaning confirmation changed; reassessment required.');
    expect(page).toContain('route.interpretation_reassessment');
    expect(page).toContain('Meaning change does not automatically alter truth.');
  });
  it('authors revision-pinned understanding and meaning confirmation without asserting truth', async () => {
    const calls=[];
    const client=new NiccoloClient(config,async(url,options)=>{calls.push({url,options});return response;});
    await client.recordResponseInterpretation('u','i','r2',{expected_current_revision_id:'r1',interpretation_text:'Meaning',rationale:'Why'});
    await client.confirmResponseInterpretation('u','i','r2','event',{state:'Confirmed',note:'Meaning only'});
    expect(calls[0].url).toContain('/epistemic/utterances/u/interpretations/i/revisions/r2');
    expect(calls[1].url).toContain('/revisions/r2/confirmations/event');
    expect(JSON.parse(calls[0].options.body).expected_current_revision_id).toBe('r1');
    expect(page).toContain('data.response_interpretations');
    expect(meaningPanel).toContain('meaning remains provisional');
    expect(meaningPanel).toContain('A new revision starts unconfirmed.');
    expect(meaningPanel).toContain('Interpretation and meaning-confirmation history');
    expect(meaningPanel).toContain('Confirmation concerns meaning, not the truth');
  });
  it('records response occurrences with provenance and independent stable identities, never as facts', async () => {
    const body = questionResponseRequest({question:'q',text:'I do not know.',speaker:'actor',evidence:'source',rationale:'Interview reply'},'utterance');
    expect(body).toEqual({utterance_id:'utterance',utterance_text:'I do not know.',speaker_actor_id:'actor',source_evidence_id:'source',source_contribution_id:null,rationale:'Interview reply'});
    const calls=[];
    const client = new NiccoloClient(config,async (url,options)=>{calls.push({url,options});return response;});
    await client.recordExpertQuestionResponse('q','response',body);
    expect(calls[0].url).toContain('/epistemic/questions/q/responses/response');
    expect(JSON.parse(calls[0].options.body)).toEqual(body);
    expect(page).toContain('pendingResponse.current?.key !== key');
    expect(page).toContain('Recorded response occurrences — not accepted facts');
    expect(page).toContain('Recording does not assert its content, validate its meaning, close a gap or send a new question.');
  });
  it('shows authored inquiry links and unrevised judgments without treating them as findings', () => {
    expect(page).toContain('data.question_inquiries');
    expect(page).toContain('inquiry.current_discriminatory_power_events');
    expect(page).toContain('Testing multiple hypotheses does not establish discriminatory power.');
    expect(page).toContain('No explicit assessment recorded.');
    expect(page).toContain('Unlinked questions remain visible');
    expect(page).toContain('Divergent judgments are not averaged or treated as consensus.');
    expect(page).toContain('inquiry.selection_criteria');
    expect(page).toContain('Reported authorization is not a permission grant');
  });
  it('records attributed inferential applicability separately from truth', async () => {
    const body = applicabilityRequest('condition', 'NOT_SATISFIED', 'Authentication failed.');
    expect(body).toEqual({condition_object_id:'condition', assessment:'NOT_SATISFIED', rationale:'Authentication failed.'});
    expect(() => applicabilityRequest('condition', 'TRUE', 'Wrong domain.')).toThrow();
    const calls = [];
    const client = new NiccoloClient(config, async (url, options) => { calls.push({url, options}); return response; });
    await client.recordExpertApplicability('argument','route','event',body);
    expect(calls[0].url).toContain('/epistemic/arguments/argument/grounding/route/applicability/event');
    expect(page).toContain('Even satisfied conditions do not establish truth or validity.');
    expect(page).toContain('No applicability condition recorded; applicability is not certified.');
  });
  it('is a primary MVP surface and says free-text interpretation is post-MVP', () => {
    expect(app).toContain('<NavLink to="/workbench">Workbench</NavLink>');
    expect(app).toContain('<Route path="/workbench" element={<ExpertWorkbenchPage/>}/>');
    expect(page).toContain('explicitly authors epistemic abstractions');
    expect(page).toContain('does not translate investigator free text into ontology');
    expect(page).toContain('<strong>Post-MVP:</strong> optional free-text interpretation');
  });

  it('uses explicit write endpoints for objects, relations, arguments and grounding', async () => {
    const calls = [];
    const client = new NiccoloClient(config, async (url, options) => { calls.push({ url, options }); return response; });
    await client.expertWorkbench();
    await client.searchExpertEvidence('approval hold');
    await client.createExpertNode('33333333-3333-4333-8333-333333333333', { kind: 'HYPOTHESIS', text: 'H', rationale: 'R' });
    await client.createExpertRelation('44444444-4444-4444-8444-444444444444', { family: 'SEMANTIC', kind: 'CONTRADICTS', left_object_id: 'a', right_object_id: 'b', rationale: 'R' });
    await client.createExpertArgument('55555555-5555-4555-8555-555555555555', { conclusion_object_id: 'a', mode: 'ABDUCTIVE', rationale: 'R' });
    await client.createExpertGrounding('55555555-5555-4555-8555-555555555555', '66666666-6666-4666-8666-666666666666', { effect: 'SUPPORTS', adequacy: 'PARTIAL', rationale: 'R', members: [] });
    await client.reviewExpertReassessment('77777777-7777-4777-8777-777777777777', '55555555-5555-4555-8555-555555555555', '88888888-8888-4888-8888-888888888888', { grounding_set_id: null, disposition: 'RETAINED', note: 'Reviewed.' });
    expect(calls.map((x) => x.options.method)).toEqual(['GET', 'GET', 'POST', 'POST', 'POST', 'POST', 'POST']);
    expect(calls[1].url).toContain('/epistemic/workbench/evidence/search?q=approval%20hold');
    expect(calls[2].url).toContain('/epistemic/objects/');
    expect(calls[3].url).toContain('/epistemic/relations/');
    expect(calls[4].url).toContain('/epistemic/arguments/');
    expect(calls[5].url).toContain('/grounding/');
    expect(calls[6].url).toContain('/epistemic/reassessment/77777777-7777-4777-8777-777777777777/arguments/55555555-5555-4555-8555-555555555555/reviews/');
  });

  it('lets a trained investigator author a proposition explicitly', () => {
    expect(nodeRequest('PROPOSITION', 'The transfer occurred after approval.', 'A proposition to be tested in this Situation.')).toEqual({
      kind: 'PROPOSITION',
      text: 'The transfer occurred after approval.',
      rationale: 'A proposition to be tested in this Situation.',
    });
  });

  it('surfaces epistemic support cycles without counting them as corroboration', () => {
    expect(page).toContain('Grounding cycle diagnostics');
    expect(page).toContain('the cycle itself adds no independent corroboration');
    expect(page).toContain('recursive support must not be treated as corroboration');
    expect(page).toContain('external grounds do not establish independence, reliability or sufficiency');
    expect(page).toContain('Context alone is not support.');
  });

  it('shows explicit reasoning routes without netting support against opposition', () => {
    expect(page).toContain('Reasoning diagnostics — structural, not a verdict');
    expect(page).toContain('does not net opposing routes into a truth or confidence score');
    expect(page).toContain('Support and opposition coexist; neither cancels the other.');
    expect(page).toContain('unanchored recursive support');
    expect(page).toContain('Shared grounds — route reuse is not independent corroboration');
    expect(page).toContain('Inspect authored grounds and rationales');
    expect(page).toContain('Exact identity reuse only; different grounds may still share provenance.');
    expect(page).toContain('Recorded documentary dependence');
    expect(page).toContain('Different components do not establish independent corroboration.');
    expect(page).toContain('Recorded lineage could not be resolved');
  });

  it('separates assessment-dependent attention from ordinary relation reassessment and truth', () => {
    expect(page).toContain('Pinned assessment revisions');
    expect(page).toContain('Downstream assessment revisions');
    expect(page).toContain('Review counts are not confidence scores.');
    expect(page).toContain('Retaining one route does not clear another.');
  });

  it('records explicit reassessment review without silently rewriting the argument', () => {
    expect(reassessmentReviewRequest(null, 'RETAINED', 'Still supported by an independent route.')).toEqual({
      grounding_set_id: null, disposition: 'RETAINED', note: 'Still supported by an independent route.',
    });
    expect(page).toContain('Latest review:');
    expect(page).toContain('Record review');
  });

  it('surfaces selective reassessment rather than invalidating the whole Situation', () => {
    expect(page).toContain('Selective reassessment impacts');
    expect(page).toContain('data.reassessment_impacts');
    expect(page).toContain('review argument');
    expect(page).toContain('downstream depth');
  });

  it('makes directed relation semantics explicit before the investigator selects endpoints', () => {
    expect(relationSemantics.SUPERSEDES).toMatchObject({ left: 'Replacement object', right: 'Superseded object' });
    expect(relationSemantics.CAUSES).toMatchObject({ left: 'Cause', right: 'Effect' });
    expect(page).toContain('relationSemantics[relation.kind].description');
    expect(page).toContain('relationSemantics[relation.kind].left');
    expect(page).toContain('relationSemantics[relation.kind].right');
  });

  it('keeps relation family semantics explicit instead of guessing them', () => {
    expect(() => relationRequest('CAUSAL', 'CONTRADICTS', 'a', 'b', 'R')).toThrow(/does not belong/);
    expect(relationRequest('SEMANTIC', 'CONTRADICTS', 'a', 'b', 'R').kind).toBe('CONTRADICTS');
  });

  it('makes Situation evidence available as an explicit grounding choice', () => {
    expect(page).toContain('Documentary evidence');
    expect(page).toContain('EVIDENCE · {item.document_locator}');
    expect(page).toContain('first 256 admissible source fragments');
    expect(page).toContain('Search above to reach material outside this bounded list.');
    expect(page).toContain('Search all attached admissible fragments');
  });


  it('requires the investigator to explain each selected ground instead of fabricating rationale', () => {
    expect(() => groundingRequest('SUPPORTS', 'PARTIAL', 'Joint reasoning', [
      { object_id: 'a', kind: 'HYPOTHESIS' },
    ], {}, [], {})).toThrow('Ground rationale');
    expect(() => groundingRequest('SUPPORTS', 'PARTIAL', 'Joint reasoning', [], {}, [
      { evidence_id: 'evidence-1' },
    ], {})).toThrow('Evidence rationale');
  });

  it('represents multiple selected objects as one joint grounding set', () => {
    const oldCrypto = globalThis.crypto;
    Object.defineProperty(globalThis, 'crypto', { configurable: true, value: { randomUUID: () => '77777777-7777-4777-8777-777777777777' } });
    try {
      const body = groundingRequest('SUPPORTS', 'PARTIAL', 'Joint reasoning', [
        { object_id: 'a', kind: 'HYPOTHESIS' },
        { object_id: 'b', kind: 'ASSUMPTION' },
      ], {}, [{ evidence_id: 'evidence-1' }], {
        'object:a': 'This hypothesis is a necessary premise for the inference.',
        'object:b': 'This assumption is being adopted explicitly for this argument.',
        'evidence:evidence-1': 'This fragment directly documents the approval hold.',
      });
      expect(body.members).toHaveLength(3);
      expect(body.members[0]).toMatchObject({ role: 'PREMISE', rationale: 'This hypothesis is a necessary premise for the inference.' });
      expect(body.members[1]).toMatchObject({ role: 'ASSUMPTION', rationale: 'This assumption is being adopted explicitly for this argument.' });
      expect(body.members[2]).toMatchObject({ role: 'EVIDENCE', ground_object_id: null, evidence_id: 'evidence-1', rationale: 'This fragment directly documents the approval hold.' });
      expect(body.adequacy).toBe('PARTIAL');
    } finally {
      Object.defineProperty(globalThis, 'crypto', { configurable: true, value: oldCrypto });
    }
  });
});
