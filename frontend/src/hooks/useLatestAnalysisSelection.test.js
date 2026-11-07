import { describe, expect, it } from 'vitest';
import { latestAnalysisId } from './useLatestAnalysisSelection.js';

describe('latest analysis selection', () => {
  it('selects the first backend history entry, which is newest-first', () => {
    expect(latestAnalysisId({ analyses: [{ analysis_id: 'new' }, { analysis_id: 'old' }] })).toBe('new');
  });
  it('does not invent a selection for empty history', () => {
    expect(latestAnalysisId({ analyses: [] })).toBe('');
    expect(latestAnalysisId(null)).toBe('');
  });
});
