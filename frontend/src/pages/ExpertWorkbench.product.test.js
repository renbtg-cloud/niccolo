import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { NiccoloClient } from '../api/client.js';
import { groundingRequest, nodeRequest, relationRequest } from '../state/expertWorkbench.js';

const app = readFileSync(new URL('../App.jsx', import.meta.url), 'utf8');
const page = readFileSync(new URL('./ExpertWorkbenchPage.jsx', import.meta.url), 'utf8');
const config = {
  workspace: '11111111-1111-4111-8111-111111111111',
  situation: '22222222-2222-4222-8222-222222222222',
  token: 'test-in-memory-credential',
};
const response = { status: 200, ok: true, headers: { get: () => null }, json: async () => ({ nodes: [], evidence: [], evidence_truncated: false, relations: [], arguments: [], grounding_sets: [] }) };

describe('expert epistemic Workbench — structured MVP authoring', () => {
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
    expect(calls.map((x) => x.options.method)).toEqual(['GET', 'GET', 'POST', 'POST', 'POST', 'POST']);
    expect(calls[1].url).toContain('/epistemic/workbench/evidence/search?q=approval%20hold');
    expect(calls[2].url).toContain('/epistemic/objects/');
    expect(calls[3].url).toContain('/epistemic/relations/');
    expect(calls[4].url).toContain('/epistemic/arguments/');
    expect(calls[5].url).toContain('/grounding/');
  });

  it('lets a trained investigator author a proposition explicitly', () => {
    expect(nodeRequest('PROPOSITION', 'The transfer occurred after approval.', 'A proposition to be tested in this Situation.')).toEqual({
      kind: 'PROPOSITION',
      text: 'The transfer occurred after approval.',
      rationale: 'A proposition to be tested in this Situation.',
    });
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
