import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { ActionDecisionPanel, decisionChoiceLabel, decisionRequest } from './ActionDecisionPanel.jsx';
import { NiccoloClient } from '../api/client.js';

const original = { recommendation_id: 'original', input: { kind: 'REVIEW_PRIVILEGES', purpose: 'Prevent exposure', conditions: 'Separate approval', uncertainty: 'Leak not established', risk: 'Exposure', alternatives: [{ kind: 'INACTION', rationale: 'Wait', expected_consequences: 'No immediate change' }], revises_recommendation_id: null } };
const successor = { recommendation_id: 'successor', input: { ...original.input, kind: 'GATHER_INFORMATION', purpose: 'Investigate further', alternatives: [{ kind: 'DELAY', rationale: 'Wait for interview', expected_consequences: 'Delay' }], revises_recommendation_id: 'original' } };
const draft = { recommendation_id: 'original', disposition: 'SELECT_ALTERNATIVE', alternative_index: 0, rationale: ' Select the recorded alternative ', revises_decision_id: null };
const recommendations = [original, successor];
const request = (value, decisions = []) => decisionRequest(value, recommendations, decisions);

describe('explicit action decision records', () => {
  it('distinguishes an inaction option from a deferred decision and pins the exact alternative', () => {
    expect(request(draft)).toEqual({ ...draft, rationale: 'Select the recorded alternative' });
    expect(decisionChoiceLabel(request(draft), original)).toBe('Alternative 1: inaction');
    expect(decisionChoiceLabel({ ...draft, disposition: 'DEFER', alternative_index: null }, original)).toBe('Defer the decision');
    expect(decisionChoiceLabel(request(draft), successor)).toBe('Alternative 1: delay');
    expect(() => request({ ...draft, alternative_index: 1 })).toThrow();
    expect(() => request({ ...draft, alternative_index: null })).toThrow();
    expect(() => request({ ...draft, disposition: 'DEFER' })).toThrow();
    expect(() => request({ ...draft, recommendation_id: 'foreign' })).toThrow();
    expect(() => request({ ...draft, rationale: '\t\u00a0' })).toThrow();
  });
  it('keeps revisions on the exact originally pinned recommendation', () => {
    const prior = { decision_id: 'old', input: request(draft) };
    const revision = { ...draft, disposition: 'DEFER', alternative_index: null, revises_decision_id: 'old' };
    expect(request(revision, [prior])).toMatchObject({ recommendation_id: 'original', revises_decision_id: 'old' });
    expect(() => request({ ...revision, recommendation_id: 'successor' }, [prior])).toThrow();
    expect(() => request(revision, [prior, { decision_id: 'new', input: request(revision, [prior]) }])).toThrow();
  });
  it('writes only an attributed choice, not authorization or execution', async () => {
    const body = request({ ...draft, authorize: true, execute: true, actual_outcome: 'not part of this record' });
    expect(body).not.toHaveProperty('authorize');
    expect(body).not.toHaveProperty('execute');
    expect(body).not.toHaveProperty('actual_outcome');
    const calls = [];
    const client = new NiccoloClient({ workspace: '11111111-1111-4111-8111-111111111111', situation: '22222222-2222-4222-8222-222222222222', token: 'test-token' }, async (url, options) => {
      calls.push({ url, options });
      return { status: 201, ok: true, headers: { get: () => null }, json: async () => ({ decision: {} }) };
    });
    await client.recordActionDecision('decision', body);
    expect(calls[0].url).toContain('/epistemic/action-decisions/decision');
    expect(calls[0].options.method).toBe('POST');
    expect(JSON.parse(calls[0].options.body)).toEqual(body);
  });
  it('renders the original choice, superseded recommendation and attributed revision history', () => {
    const prior = { decision_id: 'old', input: request(draft), recorded_by: 'principal-a', recorded_at: 'yesterday' };
    const revision = { decision_id: 'new', input: request({ ...draft, disposition: 'DEFER', alternative_index: null, revises_decision_id: 'old' }, [prior]), recorded_by: 'principal-a', recorded_at: 'today' };
    const markup = renderToStaticMarkup(createElement(ActionDecisionPanel, { snapshot: { action_recommendations: recommendations, action_decisions: [prior, revision] }, client: {}, onChanged: () => {} }));
    expect(markup).toContain('Alternative 1: inaction');
    expect(markup).toContain('Recommendation subsequently revised; original choice retained, not rebound to successor');
    expect(markup).toContain('Historical decision — explicitly revised; original retained.');
    expect(markup).toContain('principal-a');
    expect(markup).toContain('Record action decision only');
    expect(markup).toContain('A decision does not grant permission');
  });
});
