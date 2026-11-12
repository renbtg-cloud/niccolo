import { describe, expect, it, vi } from 'vitest';
import { redeemLocalHandoff, takeLocalHandoffFromFragment } from './localHandoff.js';

const code = 'a'.repeat(64);
const token = `ncl_${'b'.repeat(64)}`;

describe('local one-shot handoff', () => {
  it('removes a valid fragment before returning its capability', () => {
    const calls = [];
    const history = { state: { keep: true }, replaceState: (...args) => calls.push(args) };
    const location = { pathname: '/', search: '?workspace=w&situation=s', hash: `#handoff=${code}` };
    expect(takeLocalHandoffFromFragment(location, history)).toBe(code);
    expect(calls).toEqual([[history.state, '', '/?workspace=w&situation=s']]);
  });

  it('ignores malformed or compound fragments without rewriting history', () => {
    const history = { replaceState: vi.fn() };
    expect(takeLocalHandoffFromFragment({ hash: '#handoff=short' }, history)).toBe('');
    expect(takeLocalHandoffFromFragment({ hash: `#handoff=${code}&extra=1` }, history)).toBe('');
    expect(history.replaceState).not.toHaveBeenCalled();
  });

  it('redeems through a same-origin no-store POST and accepts only bearer-shaped output', async () => {
    const fetchImpl = vi.fn(async () => ({ ok: true, json: async () => ({ token }) }));
    await expect(redeemLocalHandoff(code, fetchImpl)).resolves.toBe(token);
    expect(fetchImpl).toHaveBeenCalledWith('/v1/local-handoff', expect.objectContaining({
      method: 'POST',
      cache: 'no-store',
      credentials: 'same-origin',
      referrerPolicy: 'no-referrer',
      body: JSON.stringify({ code }),
    }));
  });

  it('rejects failed or malformed redemptions', async () => {
    await expect(redeemLocalHandoff('x', vi.fn())).rejects.toThrow('invalid local handoff');
    await expect(redeemLocalHandoff(code, async () => ({ ok: false }))).rejects.toThrow('local handoff unavailable');
    await expect(redeemLocalHandoff(code, async () => ({ ok: true, json: async () => ({ token: 'nope' }) }))).rejects.toThrow('invalid local handoff response');
  });
});
