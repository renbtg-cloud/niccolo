import { describe, expect, it } from 'vitest';
import { qk } from './keys.js';

describe('runtime readiness query isolation', () => {
  it('shares readiness only within the same authenticated session generation', () => {
    expect(qk.readiness(7)).toEqual(qk.readiness(7));
    expect(qk.readiness(7)).not.toEqual(qk.readiness(8));
    expect(qk.readiness(7)).toEqual(['mvp-runtime-readiness', 7]);
  });

  it('keeps readiness separate from workspace-scoped data', () => {
    const workspace = { workspace: 'workspace-A', situation: 'situation-A' };
    expect(qk.readiness(7)).not.toEqual(qk.situation(workspace));
    expect(qk.readiness(7)).not.toEqual(qk.sources(workspace));
  });
});
