import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { ActionRecommendationPanel, recommendationKinds, recommendationRequest } from './ActionRecommendationPanel.jsx';
import { NiccoloClient } from '../api/client.js';

const actors = [{ actor_id: 'a', display_label: 'Reviewer' }, { actor_id: 'b', display_label: 'Another reviewer' }];
const grounds = [{ object_id: 'g', kind: 'PROPOSITION', text: 'Privileges remain broad' }];
const draft = {
  kind: 'REVIEW_PRIVILEGES', recipient_actor_id: 'a', ground_object_ids: ['g'],
  purpose: ' Protect data ', context: 'Possible leak', rationale: 'Structural vulnerability independent of suspicion',
  conditions: 'Separate human authorization', expected_consequences: 'Better visibility', uncertainty: 'No proof of leak', risk: 'Possible investigative exposure',
  reversibility: 'REVERSIBLE', revises_recommendation_id: null,
  alternatives: [{ kind: 'INACTION', rationale: 'Wait', expected_consequences: 'Existing exposure persists' }],
};
const request = (input, history = []) => recommendationRequest(input, actors, grounds, history);

describe('non-executable action recommendation records', () => {
  it('requires structured grounds, uncertainty, risk and a distinct alternative', () => {
    expect(recommendationKinds).not.toContain('KINETIC_ATTACK');
    expect(request(draft)).toEqual({ ...draft, purpose: 'Protect data' });
    expect(() => request({ ...draft, ground_object_ids: [] })).toThrow();
    expect(() => request({ ...draft, ground_object_ids: ['g', 'g'] })).toThrow();
    expect(() => request({ ...draft, ground_object_ids: ['foreign'] })).toThrow();
    expect(() => request({ ...draft, recipient_actor_id: 'foreign' })).toThrow();
    expect(() => request({ ...draft, risk: '\u00a0\u0085' })).toThrow();
    expect(() => request({ ...draft, uncertainty: 'é'.repeat(2049) })).toThrow();
    expect(() => request({ ...draft, alternatives: [] })).toThrow();
    expect(() => request({ ...draft, alternatives: [{ ...draft.alternatives[0], kind: draft.kind }] })).toThrow();
    expect(() => request({ ...draft, kind: 'KINETIC_ATTACK' })).toThrow();
  });
  it('pins revisions to an unrevised record addressed to the same registered recipient', () => {
    const prior = { recommendation_id: 'old', input: request(draft) };
    const revised = { ...draft, revises_recommendation_id: 'old', risk: 'Review discreetly' };
    expect(request(revised, [prior])).toMatchObject({ revises_recommendation_id: 'old', risk: 'Review discreetly' });
    expect(() => request({ ...revised, recipient_actor_id: 'b' }, [prior])).toThrow();
    expect(() => request(revised)).toThrow();
    expect(() => request(revised, [prior, { recommendation_id: 'new', input: request(revised, [prior]) }])).toThrow();
  });
  it('includes no execution or permission fields and uses the advisory record endpoint', async () => {
    const body = request({ ...draft, execute: true, authorized: true });
    expect(body).not.toHaveProperty('execute');
    expect(body).not.toHaveProperty('authorized');
    const calls = [];
    const client = new NiccoloClient({ workspace: '11111111-1111-4111-8111-111111111111', situation: '22222222-2222-4222-8222-222222222222', token: 'test-token' }, async (url, options) => {
      calls.push({ url, options });
      return { status: 201, ok: true, headers: { get: () => null }, json: async () => ({ recommendation: {} }) };
    });
    await client.recordActionRecommendation('recommendation', body);
    expect(calls[0].url).toContain('/epistemic/action-recommendations/recommendation');
    expect(calls[0].options.method).toBe('POST');
    expect(JSON.parse(calls[0].options.body)).toEqual(body);
  });
  it('renders attribution, original history, explicit grounds and non-execution language', () => {
    const original = { recommendation_id: 'old', input: request(draft), recorded_by: 'human-a', recorded_at: 'yesterday' };
    const successor = { recommendation_id: 'new', input: request({ ...draft, revises_recommendation_id: 'old' }, [original]), recorded_by: 'human-b', recorded_at: 'today' };
    const html = renderToStaticMarkup(createElement(ActionRecommendationPanel, { snapshot: { nodes: grounds, action_recommendations: [original, successor] }, actors, client: {}, onChanged: () => {} }));
    expect(html).toContain('Record recommendation only');
    expect(html).toContain('Recording does not make a decision, grant permission, send a message');
    expect(html).toContain('Historical — explicitly revised; original retained.');
    expect(html).toContain('human-a');
    expect(html).toContain('human-b');
    expect(html).toContain('Privileges remain broad');
    expect(html).toContain('Expected consequences, not observed outcomes');
  });
  it('preserves optional qualified groups and rejects incomplete or duplicate coverage', () => {
    const moreGrounds = [...grounds, {object_id: 's', kind: 'HYPOTHESIS', text: 'Suspected leak'}];
    const qualified = { ...draft, ground_object_ids: ['g', 's'], qualified_basis: [{kind: 'ALTERNATIVE_SUFFICIENT', ground_object_ids: ['g'], rationale: 'Structural risk'}, {kind: 'CONTEXT_ONLY', ground_object_ids: ['s'], rationale: 'Motivating suspicion'}] };
    const build = input => recommendationRequest(input, actors, moreGrounds, []);
    expect(build(qualified).qualified_basis).toEqual(qualified.qualified_basis);
    expect(() => build({...qualified, qualified_basis: qualified.qualified_basis.slice(0, 1)})).toThrow();
    expect(() => build({...qualified, qualified_basis: [{kind: 'COMPLEMENTARY', ground_object_ids: ['s', 's', 'g'], rationale: 'Duplicate'}]})).toThrow();
    expect(() => build({...qualified, qualified_basis: [{kind: 'COMPLEMENTARY', ground_object_ids: ['other'], rationale: 'Foreign'}]})).toThrow();
    expect(build({...qualified, qualified_basis: []})).not.toHaveProperty('qualified_basis');
  });
  it('renders affected groups and alternatives not linked to a change without deciding appropriateness', () => {
    const moreGrounds = [...grounds, {object_id: 's', kind: 'HYPOTHESIS', text: 'Suspected leak'}];
    const qualified = { ...draft, ground_object_ids: ['g', 's'], qualified_basis: [{kind: 'ALTERNATIVE_SUFFICIENT', ground_object_ids: ['g'], rationale: 'Structural risk'}, {kind: 'CONTEXT_ONLY', ground_object_ids: ['s'], rationale: 'Motivating suspicion'}] };
    const record = {recommendation_id: 'rec', input: recommendationRequest(qualified, actors, moreGrounds, []), recorded_by: 'author', recorded_at: 'today'};
    const impact = {recommendation_id: 'rec', trigger_relation_id: 'revision', affected_ground_object_ids: ['s'], affected_basis_indices: [1], alternative_basis_indices_not_linked_to_trigger: [0], grounds_unqualified: false, historical_recommendation: false};
    const html = renderToStaticMarkup(createElement(ActionRecommendationPanel, {snapshot: {nodes: moreGrounds, action_recommendations: [record], recommendation_basis_impacts: [impact]}, actors, client: {}, onChanged: () => {}}));
    expect(html).toContain('Basis 1: Declared alternatively sufficient');
    expect(html).toContain('Affected groups: 2');
    expect(html).toContain('Declared sufficient alternatives not linked to this trigger: 1');
    expect(html).toContain('does not confirm independence, current adequacy or appropriateness');
    expect(html).toContain('reconsider appropriateness without automatically invalidating the recommendation');
  });
});
