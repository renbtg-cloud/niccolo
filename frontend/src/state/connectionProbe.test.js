import { describe, expect, it } from 'vitest';
import { ApiError } from '../api/client.js';
import { connectionFailureMessage, validateConnection } from './connectionProbe.js';

function response(status, value = {}) {
  return {
    status,
    ok: status >= 200 && status < 300,
    headers: { get: () => null },
    json: async () => value,
  };
}

const config = {
  workspace: '11111111-1111-4111-8111-111111111111',
  situation: '22222222-2222-4222-8222-222222222222',
  token: 'private-token',
};

describe('connection validation', () => {
  it('probes a bounded workspace read without requiring the prepared Situation to exist', async () => {
    const calls = [];
    await expect(validateConnection(config, async (url, options) => {
      calls.push([url, options]);
      return response(200, { documents: [], next_after: null });
    })).resolves.toBe(true);
    expect(calls).toHaveLength(1);
    expect(calls[0][0]).toContain('/v1/workspaces/11111111-1111-4111-8111-111111111111/sources?limit=1');
    expect(calls[0][1].headers.Authorization).toBe('Bearer private-token');
    expect(calls[0][1].cache).toBe('no-store');
  });

  it('does not convert rejected credentials into a connected session', async () => {
    await expect(validateConnection(config, async () => response(401, { error: 'invalid bearer token' })))
      .rejects.toMatchObject({ status: 401 });
  });

  it('turns common authentication failures into first-user language', () => {
    expect(connectionFailureMessage(new ApiError(401, 'no'))).toMatch(/token rejected/i);
    expect(connectionFailureMessage(new ApiError(403, 'no'))).toMatch(/does not grant read access/i);
    expect(connectionFailureMessage(new TypeError('fetch failed'))).toMatch(/service is running/i);
  });
});
