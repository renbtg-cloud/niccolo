import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { NiccoloClient } from '../api/client.js';

const app = readFileSync(new URL('../App.jsx', import.meta.url), 'utf8');
const page = readFileSync(new URL('./EpistemicPlaygroundPage.jsx', import.meta.url), 'utf8');
const config = {
  workspace: '11111111-1111-4111-8111-111111111111',
  situation: '22222222-2222-4222-8222-222222222222',
  token: 'test-in-memory-credential',
};
const response = { status: 200, ok: true, headers: { get: () => null },
  json: async () => ({ effective_policy: { scope: 'BUILTIN_DEFAULT' } }) };

describe('epistemic Playground — real API contract without state mutation', () => {
  it('routes under Technical, not the primary Situation → Analyze → Review journey', () => {
    expect(app).toContain("['/playground','Playground']");
    expect(app).toContain('<Route path="/playground" element={<EpistemicPlaygroundPage/>}/>');
    expect(app).toContain('<details className="advanced-nav">');
  });
  it('labels fictional, nonpersistent arithmetic and never claims a full LLM replay', () => {
    expect(page).toContain('Compare without saving');
    expect(page).toContain('not a full reasoning replay');
    expect(page).toContain('not probabilities of truth');
    expect(page).toContain('same fictional source');
    expect(page).toContain('not checked against documents');
  });
  it('uses GET for current policy and POST only for nonpersistent comparison', async () => {
    const calls = [];
    const client = new NiccoloClient(config, async (url, options) => {
      calls.push({ url, options }); return response;
    });
    await client.epistemicPolicy();
    await client.compareEpistemic({ trial_policy: { support_multiplier_pct: 100 } });
    expect(calls[0].options.method).toBe('GET');
    expect(calls[0].url).toContain('/epistemic/policy?situation_id=' + config.situation);
    expect(calls[1].options.method).toBe('POST');
    expect(calls[1].url).toContain('/epistemic/compare');
    expect(calls[1].options.cache).toBe('no-store');
    expect(calls[1].options.body).toContain('trial_policy');
    expect(calls[0].url + calls[1].url).not.toContain(config.token);
  });
});
