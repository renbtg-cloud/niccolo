import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { InterventionPanel, interventionRequest, causalAssessmentRequest, causalReviewRequest } from './InterventionPanel.jsx';
import { NiccoloClient } from '../api/client.js';

const draft = { kind: 'EXECUTION_OCCURRENCE', linked_decision_id: null, characterization: 'REPORTED', description: 'Manager reports access review', event_temporal_scope: 'January', acquisition_temporal_scope: 'March', basis_evidence_id: null, revises_record_id: null };
const observation = { ...draft, kind: 'EVENT_OBSERVATION', description: 'Employee explores other work' };
const row = (id, input) => ({ id, input, recorded_by: 'author', recorded_at: 'today' });
const records = [row('execution', draft), row('observation', observation)];
const causal = { execution_record_id: 'execution', event_observation_record_id: 'observation', assessment: 'POSSIBLY_INDUCED', rationale: 'Reported temporal proximity', alternative_explanations: 'Prior plans', uncertainty: 'Unknown onset', basis_evidence_id: null, revises_assessment_id: null };
const record = (value, current = records) => interventionRequest(value, current, [{decision_id: 'decision'}], [{evidence_id: 'basis'}]);
const assess = (value, current = []) => causalAssessmentRequest(value, records, current, []);

describe('intervention and causal assessment authoring', () => {
  it('separates reports from observed characterization and validates source and decision pins', () => {
    expect(record(draft)).toEqual(draft);
    expect(() => record({ ...draft, characterization: 'OBSERVED' })).toThrow();
    expect(record({ ...draft, characterization: 'OBSERVED', basis_evidence_id: 'basis' })).toMatchObject({ basis_evidence_id: 'basis' });
    expect(() => record({ ...draft, linked_decision_id: 'foreign' })).toThrow();
    expect(() => record({ ...draft, acquisition_temporal_scope: ' ' })).toThrow();
    expect(record({ ...draft, execute: true, authorize: true })).toEqual(draft);
  });
  it('preserves the original stage and decision pin on a revision', () => {
    const revision = { ...draft, revises_record_id: 'execution', characterization: 'CONTESTED' };
    expect(record(revision)).toEqual(revision);
    expect(() => record({ ...revision, kind: 'AUTHORIZATION_REPORT' })).toThrow();
    expect(() => record({ ...revision, linked_decision_id: 'decision' })).toThrow();
    expect(() => record(revision, [...records, row('revised', revision)])).toThrow();
  });
  it('requires distinct causal pins and an explicit rationale, alternatives and uncertainty', () => {
    expect(assess(causal)).toEqual(causal);
    expect(() => assess({ ...causal, execution_record_id: 'observation' })).toThrow();
    expect(() => assess({ ...causal, alternative_explanations: '' })).toThrow();
    expect(() => assess({ ...causal, uncertainty: '' })).toThrow();
    const revision = { ...causal, assessment: 'UNDETERMINED', revises_assessment_id: 'assessment' };
    const prior = row('assessment', causal);
    expect(assess(revision, [prior])).toEqual(revision);
    expect(() => assess(revision, [prior, row('already-revised', revision)])).toThrow();
  });
  it('sends descriptions and assessments through independent authenticated authoring endpoints', async () => {
    const calls = [];
    const client = new NiccoloClient({workspace: 'workspace', situation: 'situation', token: 'token'}, async (url, options) => { calls.push({url, options}); return {status: 201, ok: true, headers: {get: () => null}, json: async () => ({})}; });
    await client.recordIntervention('execution', record(draft));
    await client.recordInterventionAssessment('assessment', assess(causal));
    expect(calls[0].url).toContain('/epistemic/intervention-records/execution');
    expect(calls[1].url).toContain('/epistemic/intervention-causal-assessments/assessment');
    expect(JSON.parse(calls[0].options.body)).toEqual(draft);
    expect(JSON.parse(calls[1].options.body)).toEqual(causal);
    expect(calls[0].options.headers.Authorization).toBe('Bearer token');
  });
  it('renders exact historical pins, attributed record history and selective review without causal certainty', () => {
    const updated = row('new-observation', {...observation, revises_record_id: 'observation', event_temporal_scope: 'December'});
    const view = {assessment: row('assessment', causal), execution_revised: false, event_observation_revised: true, requires_review: true, superseded: false, current_execution_record_id: 'execution', current_event_record_id: 'new-observation'};
    const markup = renderToStaticMarkup(createElement(InterventionPanel, {snapshot: {intervention_records: [...records, updated], intervention_causal_assessments: [view]}, client: {}, onChanged: () => {}}));
    expect(markup).toContain('Sequence alone does not establish causation');
    expect(markup).toContain('Historical record — original retained');
    expect(markup).toContain('Pinned record revised; review required');
    expect(markup).toContain('Employee explores other work · historical · observation');
    expect(markup).toContain('Record causal assessment only');
    expect(markup).toContain('Recorded by author');
    expect(markup).toContain('Record causal basis review only');
    expect(markup).toContain('Current event revision: Employee explores other work · current · new-observation');
  });
  it('pins reviews to the current changed basis and renders previous review history', async () => {
    const view = {assessment: row('assessment', causal), execution_revised: false, event_observation_revised: true, requires_review: false, superseded: false, current_execution_record_id: 'execution', current_event_record_id: 'revised'};
    const body = causalReviewRequest({disposition: 'RETAINED', note: 'Tentative judgment remains justified'}, view);
    expect(body).toEqual({assessment_id: 'assessment', reviewed_execution_record_id: 'execution', reviewed_event_record_id: 'revised', disposition: 'RETAINED', note: 'Tentative judgment remains justified'});
    expect(() => causalReviewRequest({disposition: 'RETAINED', note: ''}, view)).toThrow();
    expect(() => causalReviewRequest({disposition: 'RETAINED', note: 'Note'}, {...view, event_observation_revised: false})).toThrow();
    const priorReview = row('review', body);
    const markup = renderToStaticMarkup(createElement(InterventionPanel, {snapshot: {intervention_records: [...records, row('revised', {...observation, revises_record_id: 'observation'})], intervention_causal_assessments: [{...view, latest_review: priorReview, review_history: [priorReview]}]}, client: {}, onChanged: () => {}}));
    expect(markup).toContain('Changed causal basis reviewed; original assessment and exact pins retained');
    expect(markup).toContain('Attributed causal review history');
    expect(markup).toContain('Tentative judgment remains justified');
    const calls = [];
    const client = new NiccoloClient({workspace: 'w', situation: 's', token: 'token'}, async (url, options) => { calls.push({url, options}); return {status: 201, ok: true, headers: {get: () => null}, json: async () => ({})}; });
    await client.recordInterventionReview('review', body);
    expect(calls[0].url).toContain('/epistemic/intervention-causal-reviews/review');
    expect(JSON.parse(calls[0].options.body)).toEqual(body);
  });
});
