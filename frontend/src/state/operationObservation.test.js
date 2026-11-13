import { describe, it, expect } from 'vitest';
import { completedAnalysisOutputRefreshId, eventPageNeedsCatchup, mergeOperationObservation, operationPhase, observationPollDelay } from './operationObservation.js';

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

  it('keeps only recent browser events without losing the server cursor', () => {
    const previous = {
      operation: { state: 'RUNNING', state_version: 2 },
      events: Array.from({ length: 500 }, (_, index) => ({
        event_seq: index + 1,
        kind: 'PROGRESS',
      })),
      cursor: 500,
    };
    const merged = mergeOperationObservation(previous, {
      operation: { state: 'RUNNING', state_version: 3 },
      events: [{ event_seq: 501, kind: 'PROGRESS' }],
      next_after_event: 501,
    });
    expect(merged.events).toHaveLength(500);
    expect(merged.events[0].event_seq).toBe(2);
    expect(merged.events.at(-1).event_seq).toBe(501);
    expect(merged.cursor).toBe(501);
    expect(merged.eventsTruncated).toBe(true);
    const next = mergeOperationObservation(merged, {
      operation: { state: 'SUCCEEDED', state_version: 4 },
      events: [],
      next_after_event: 501,
    });
    expect(next.eventsTruncated).toBe(true);
    expect(next.cursor).toBe(501);
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

  it('keeps polling when an event page is full because more terminal events may exist', () => {
    expect(eventPageNeedsCatchup({ events: Array.from({ length: 100 }, () => ({})) }, 100)).toBe(true);
    expect(eventPageNeedsCatchup({ events: Array.from({ length: 99 }, () => ({})) }, 100)).toBe(false);
    expect(eventPageNeedsCatchup({ events: [] }, 100)).toBe(false);
    expect(eventPageNeedsCatchup(null, 100)).toBe(false);
    expect(eventPageNeedsCatchup({ events: [{}] }, 0)).toBe(false);
  });

  it('avoids tight polling and stops on terminal operations', () => {
    expect(observationPollDelay(undefined)).toBe(false);
    expect(observationPollDelay('QUEUED')).toBe(250);
    expect(observationPollDelay('RUNNING')).toBe(250);
    expect(observationPollDelay('WAITING_FOR_HUMAN')).toBe(1000);
    for (const state of ['SUCCEEDED', 'FAILED', 'CANCELLED']) {
      expect(observationPollDelay(state)).toBe(false);
    }
  });

  it('requests one final output refresh when a live analysis succeeds', () => {
    const analysis = '33333333-3333-4333-8333-333333333333';
    const previous = { operation: { state: 'RUNNING', state_version: 7, target_analysis_id: analysis } };
    const succeeded = { operation: { state: 'SUCCEEDED', state_version: 8, target_analysis_id: analysis } };
    expect(completedAnalysisOutputRefreshId(previous, succeeded)).toBe(analysis);
    expect(completedAnalysisOutputRefreshId(succeeded, succeeded)).toBe(null);
    expect(completedAnalysisOutputRefreshId(null, succeeded)).toBe(analysis);
    expect(completedAnalysisOutputRefreshId(previous, { operation: { state: 'WAITING_FOR_HUMAN', target_analysis_id: analysis } })).toBe(null);
  });

  it('reports transport and terminal phases explicitly', () => {
    expect(operationPhase(null, { pending: true })).toBe('loading');
    expect(operationPhase(null, { error: new Error('x') })).toBe('error');
    expect(operationPhase({ operation: { state: 'SUCCEEDED' } }, {})).toBe('terminal');
    expect(operationPhase({ operation: { state: 'RUNNING' } }, { fetching: true })).toBe('observing');
  });
});
