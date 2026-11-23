import { QueryClient } from '@tanstack/react-query';
import { expect, it } from 'vitest';
import { invalidateSituationReview, qk } from './keys.js';

it('marks past analysis and live leads stale after a Situation edit', async () => {
  const cache = new QueryClient();
  const current = { workspace: 'w', situation: 's' };
  const other = { workspace: 'w', situation: 'other' };
  const affected = [
    qk.situation(current),
    qk.analysis(current, 'past', 'review'),
    qk.dashboard(current, 'past', null),
    qk.dashboard(current, 'past', 'op'),
    qk.analysisHistory(current),
  ];
  const unaffected = qk.dashboard(other, 'past', null);
  for (const key of [...affected, unaffected]) cache.setQueryData(key, {});
  await invalidateSituationReview(cache, current);
  expect(affected.every(key => cache.getQueryState(key).isInvalidated)).toBe(true);
  expect(cache.getQueryState(unaffected).isInvalidated).toBe(false);
});
