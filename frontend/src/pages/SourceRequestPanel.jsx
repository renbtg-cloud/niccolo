import { useMutation, useQuery } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { qk } from '../api/keys.js';
import { isCurrentSession } from '../stores/session.js';
import { sourceRequestActions, sourceRequestEventRequest } from '../state/sourceRequestDecision.js';

const labels = {
  ACCEPTED: 'Accept request',
  DEFERRED: 'Defer request',
  DECLINED: 'Decline request',
  REOPENED: 'Reopen request',
  COMPLETED: 'Mark acquisition complete',
};

function SourceRequestDecisionCard({ request, client, analysis, generation, onChanged }) {
  const pending = useRef(null);
  const actions = sourceRequestActions(request.state);
  const [kind, setKind] = useState(actions[0]);
  const [note, setNote] = useState('');
  const decision = useMutation({
    mutationFn: async () => {
      const body = sourceRequestEventRequest(kind, note);
      const signature = JSON.stringify({ analysis, request: request.request_id, ...body });
      if (pending.current?.signature !== signature) pending.current = { signature, id: crypto.randomUUID() };
      return (await client.recordSourceRequestEvent(
        analysis,
        request.request_id,
        pending.current.id,
        body.kind,
        body.note,
      )).value;
    },
    onSuccess: async () => {
      if (!isCurrentSession(generation)) return;
      pending.current = null;
      setNote('');
      await onChanged();
    },
    onError: async error => {
      if (isCurrentSession(generation) && error.status === 409) await onChanged();
    },
  });
  return <li>
    <strong>{request.text}</strong>
    <p>{request.rationale}</p>
    <small>{request.source_kind} · current state: {request.state.toLowerCase()} · proposed by <code>{request.proposed_by}</code> at {request.proposed_at}</small>
    {request.events.length > 0 && <details><summary>Attributed decision history ({request.events.length})</summary><ul>{request.events.map(event => <li key={event.event_id}><strong>Decision #{event.event_no}: {event.kind.toLowerCase()}</strong> — {event.note}<div><small>Recorded by <code>{event.recorded_by}</code> at {event.recorded_at}</small></div></li>)}</ul></details>}
    <form onSubmit={event => { event.preventDefault(); decision.mutate(); }}>
      <fieldset disabled={decision.isPending}>
        <legend>Record your decision</legend>
        <label>Decision<select value={kind} onChange={event => setKind(event.target.value)}>{actions.map(action => <option key={action} value={action}>{labels[action]}</option>)}</select></label>
        <label>Why<textarea required maxLength={4096} value={note} onChange={event => setNote(event.target.value)} placeholder="Your reason is attributed human context."/></label>
        <button type="submit">Record decision only</button>
      </fieldset>
    </form>
    {decision.isSuccess && <p role="status">Source-request decision recorded.</p>}
    {decision.error && <p role="alert">Decision unavailable: {decision.error.message}</p>}
  </li>;
}

export function SourceRequestPanel({ client, analysis, generation, onChanged }) {
  const requests = useQuery({
    queryKey: qk.analysis(client.config, analysis, 'source-requests'),
    queryFn: async ({ signal }) => (await client.sourceRequests(analysis, signal)).value,
  });
  return <article className="review-panel"><h3>Requests for more source material</h3>
    <p>These are suggestions for you. Accepting one records your decision only: Niccolò does not fetch messages, files or links.</p>
    {requests.isPending ? <p role="status">Loading source requests…</p>
      : requests.error ? <div><p role="alert">Source requests unavailable: {requests.error.message}</p><button type="button" disabled={requests.isFetching} onClick={() => void requests.refetch()}>Retry source requests</button></div>
      : requests.data.source_requests.length ? <ul>{requests.data.source_requests.map(request => <SourceRequestDecisionCard key={`${request.request_id}:${request.state}`} request={request} client={client} analysis={analysis} generation={generation} onChanged={onChanged}/>)}</ul>
      : <p>No source request is recorded for this analysis.</p>}
    <small>The external action remains yours. A recorded acceptance never means that material was fetched. Marking acquisition complete is your attributed report; it does not prove that material exists, attach it or make it evidence.</small>
  </article>;
}
