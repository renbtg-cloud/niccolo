import { describe, it, expect } from 'vitest';
import { mergeOperationObservation, operationPhase } from './operationObservation.js';

describe('operation observation state', () => {
  it('never regresses operation version and deduplicates events', () => {
    const previous = {
      operation: { state: 'RUNNING', state_version: 7 },
      events: [{ event_seq: 2, kind: 'STARTED' }],
      cursor: 2,
    };
    const merged = mergeOperationObservation(previous, {
      operation: { state: 'QUEUED', state_version: 6 },
      events: [
        { event_seq: 2, kind: 'STARTED' },
        { event_seq: 3, kind: 'PROGRESS' },
      ],
      next_after_event: 3,
      operation_changed: false,
      events_changed: true,
      timed_out: false,
    });
    expect(merged.operation.state_version).toBe(7);
    expect(merged.events.map((e) => e.event_seq)).toEqual([2, 3]);
    expect(merged.cursor).toBe(3);
  });

  it('advances operation and cursor monotonically', () => {
    const merged = mergeOperationObservation(
      { operation: { state: 'RUNNING', state_version: 2 }, events: [], cursor: 10 },
      { operation: { state: 'WAITING_FOR_HUMAN', state_version: 3 }, events: [], next_after_event: 8 },
    );
    expect(merged.operation.state).toBe('WAITING_FOR_HUMAN');
    expect(merged.cursor).toBe(10);
    expect(operationPhase(merged, {})).toBe('waiting-human');
  });

  it('reports transport and terminal phases explicitly', () => {
    expect(operationPhase(null, { pending: true })).toBe('loading');
    expect(operationPhase(null, { error: new Error('x') })).toBe('error');
    expect(operationPhase({ operation: { state: 'SUCCEEDED' } }, {})).toBe('terminal');
    expect(operationPhase({ operation: { state: 'RUNNING' } }, { fetching: true })).toBe('observing');
  });
});
