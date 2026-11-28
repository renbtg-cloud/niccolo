import { useRef, useState } from 'react';
import { useMutation } from '@tanstack/react-query';

const encoder = new TextEncoder();

export function SupportRouteDecision({route, label, suspended, history, onRecord, onSaved, allowSuspension = true}) {
  const [disposition, setDisposition] = useState('');
  const [rationale, setRationale] = useState('');
  const pending = useRef(null);
  const note = rationale.trim();
  const ready = disposition && (disposition !== 'SUSPEND' || allowSuspension)
    && note && encoder.encode(note).length <= 4096;
  const mutation = useMutation({
    mutationFn: (body) => {
      const signature = JSON.stringify(body);
      if (pending.current?.signature !== signature) pending.current = {signature, id: crypto.randomUUID()};
      return onRecord(pending.current.id, body);
    },
    onSuccess: async () => {
      pending.current = null;
      setDisposition('');
      setRationale('');
      await onSaved();
    },
  });
  return <li>
    <strong>{label}</strong> · {suspended ? 'Suspended' : 'In use'}
    <form onSubmit={(event) => {
      event.preventDefault();
      if (mutation.isPending || !ready) return;
      mutation.mutate({
        argument_id: route.argument_id, grounding_set_id: route.grounding_set_id,
        disposition, rationale: note,
      });
    }}>
      <label>Use of this route<select value={disposition} onChange={(event) => setDisposition(event.target.value)} required>
        <option value="">Choose…</option><option value="USE">{suspended ? 'Restore use' : 'Keep in use'}</option>
        {allowSuspension && <option value="SUSPEND">Suspend use</option>}
      </select></label>
      <label>Justification<input value={rationale} onChange={(event) => setRationale(event.target.value)} maxLength={4096} required/></label>
      <button disabled={mutation.isPending || !ready} type="submit">Record route decision</button>
      {mutation.error && <p className="error" role="alert">{mutation.error.message}</p>}
    </form>
    {history.length > 0 && <details><summary>Route decision history</summary><ul>{history.map((decision) => <li key={decision.event_id}>
      {decision.disposition === 'SUSPEND' ? 'Suspended' : 'Use recorded'} · {decision.rationale}<br/>
      <small>Recorded by {decision.recorded_by} · {decision.recorded_at}</small>
    </li>)}</ul></details>}
  </li>;
}
