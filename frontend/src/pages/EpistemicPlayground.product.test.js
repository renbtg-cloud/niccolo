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
    expect(page).toContain('Not a full reasoning replay');
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
    await client.recordEpistemicPolicy('33333333-3333-4333-8333-333333333333', {
      situation_id: config.situation,
      policy: { support_multiplier_pct: 150, counter_multiplier_pct: 100, per_origin_cap: 100 },
      rationale: 'Human reviewed the hypothetical changes',
    });
    expect(calls[0].options.method).toBe('GET');
    expect(calls[0].url).toContain('/epistemic/policy?situation_id=' + config.situation);
    expect(calls[1].options.method).toBe('POST');
    expect(calls[1].url).toContain('/epistemic/compare');
    expect(calls[1].options.cache).toBe('no-store');
    expect(calls[1].options.body).toContain('trial_policy');
    expect(calls[2].options.method).toBe('POST');
    expect(calls[2].url).toContain('/epistemic/policies/33333333-3333-4333-8333-333333333333');
    expect(JSON.parse(calls[2].options.body).rationale).toContain('Human reviewed');
    expect(calls[2].options.headers.Authorization).toContain(config.token);
    expect(calls.map((call) => call.url).join(' ')).not.toContain(config.token);
  });
  it('never saves on a preview and requires an explicit rationale and scope choice', () => {
    expect(page).toContain('Record a new policy version');
    expect(page).toContain('policyScope ===');
    expect(page).toContain('rationale,');
    expect(page).toContain('onClick={saveTrial}');
    expect(page).toContain('it does not rewrite frozen analyses');
    expect(page).toContain('Policy recorded successfully.');
    expect(page).toContain('save.isSuccess');
  });
});
