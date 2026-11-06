export const TERMINAL_OPERATION_STATES = new Set(['SUCCEEDED', 'FAILED', 'CANCELLED']);

export function mergeOperationObservation(previous, incoming) {
  if (!incoming) return previous ?? null;
  const prior = previous ?? { operation: null, events: [], cursor: 0 };
  const priorVersion = prior.operation?.state_version ?? -1;
  const incomingOperation = incoming.operation ?? null;
  const incomingVersion = incomingOperation?.state_version ?? -1;
  const operation = incomingVersion >= priorVersion ? incomingOperation : prior.operation;

  const bySeq = new Map();
  for (const event of prior.events ?? []) bySeq.set(event.event_seq, event);
  for (const event of incoming.events ?? []) bySeq.set(event.event_seq, event);
  const events = [...bySeq.values()].sort((a, b) => a.event_seq - b.event_seq);
  const observedCursor = incoming.next_after_event ?? events.at(-1)?.event_seq ?? 0;
  const cursor = Math.max(prior.cursor ?? 0, observedCursor);

  return {
    operation,
    events,
    cursor,
    operationChanged: Boolean(incoming.operation_changed),
    eventsChanged: Boolean(incoming.events_changed),
    timedOut: Boolean(incoming.timed_out),
  };
}

export function operationPhase(observation, transport = {}) {
  if (transport.error) return 'error';
  const operation = observation?.operation;
  if (!operation) return transport.pending ? 'loading' : 'idle';
  if (operation.state === 'WAITING_FOR_HUMAN') return 'waiting-human';
  if (TERMINAL_OPERATION_STATES.has(operation.state)) return 'terminal';
  if (transport.fetching) return 'observing';
  return 'active';
}
