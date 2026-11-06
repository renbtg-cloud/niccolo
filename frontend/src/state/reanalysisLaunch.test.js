import { describe, expect, it } from 'vitest';
import { launchApprovedReanalysis } from './reanalysisLaunch.js';

describe('human-approved reanalysis launch', () => {
  it('creates preserved lineage before starting agentic work', async () => {
    const calls = [];
    const client = {
      async runReanalysisSync(operation, prior, next, goal) {
        calls.push(['snapshot', operation, prior, next, goal]);
        return { value: { result: { new_analysis_id: next, historical_output_preserved: true } } };
      },
      async runIntentAsync(operation, body) {
        calls.push(['agentic', operation, body]);
        return { value: { operation: { operation_id: operation, state: 'QUEUED' } } };
      },
    };
    const ids = ['snapshot-op', 'new-analysis', 'agentic-op'];
    const result = await launchApprovedReanalysis({
      client,
      priorAnalysis: 'old-analysis',
      goal: ' Revisit with new source ',
      uuid: () => ids.shift(),
    });
    expect(calls[0]).toEqual(['snapshot','snapshot-op','old-analysis','new-analysis','Revisit with new source']);
    expect(calls[1][2]).toMatchObject({
      analysis_id: 'new-analysis',
      reanalysis_of: 'old-analysis',
      intent: 'UNDERSTAND',
      goal: 'Revisit with new source',
      context: { surface: 'review-reanalysis', human_approved: true },
    });
    expect(result).toMatchObject({ newAnalysis: 'new-analysis', agenticOperation: 'agentic-op' });
  });

  it('never launches agentic work if preservation contract fails', async () => {
    let agenticCalled = false;
    const client = {
      async runReanalysisSync() { return { value: { result: { historical_output_preserved: false } } }; },
      async runIntentAsync() { agenticCalled = true; },
    };
    await expect(launchApprovedReanalysis({ client, priorAnalysis: 'old', goal: 'again', uuid: () => 'x' })).rejects.toThrow('not preserved');
    expect(agenticCalled).toBe(false);
  });
});
