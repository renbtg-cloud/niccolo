import { useRef, useState } from 'react';
import { useMutation } from '@tanstack/react-query';
import { requireText } from '../state/expertWorkbench.js';

export function ResponseInterpretationPanel({utterance, view, client, onChanged}) {
  const current = view?.revisions.find(r => r.revision_id === view.current_revision_id);
  const [draft,setDraft] = useState({text:'',rationale:''});
  const [confirmation,setConfirmation] = useState({state:'Confirmed',note:''});
  const [error,setError] = useState('');
  const pendingRevision = useRef(null);
  const pendingConfirmation = useRef(null);
  const record = useMutation({mutationFn: ({id,revision,body}) => client.recordResponseInterpretation(utterance.utterance_id,id,revision,body),onSuccess:async()=>{pendingRevision.current=null;setDraft({text:'',rationale:''});await onChanged();}});
  const confirm = useMutation({mutationFn:({id,revision,event,body}) => client.confirmResponseInterpretation(utterance.utterance_id,id,revision,event,body),onSuccess:async()=>{pendingConfirmation.current=null;setConfirmation({state:'Confirmed',note:''});await onChanged();}});
  const busy = record.isPending || confirm.isPending;
  const revisionSubmit = e => {
    e.preventDefault();setError('');
    try {
      const body={expected_current_revision_id:current?.revision_id ?? null,interpretation_text:requireText(draft.text,'Derived interpretation',8192),rationale:requireText(draft.rationale,'Interpretation rationale')};
      const key=JSON.stringify([view?.interpretation_id,body]);
      if(pendingRevision.current?.key!==key) pendingRevision.current={key,id:view?.interpretation_id ?? crypto.randomUUID(),revision:crypto.randomUUID()};
      record.mutate({...pendingRevision.current,body});
    } catch(e) {setError(e.message);}
  };
  const confirmationSubmit=e=>{
    e.preventDefault();setError('');
    try {
      const body={state:confirmation.state,note:requireText(confirmation.note,'Confirmation note',2048)};
      const key=JSON.stringify([view.interpretation_id,current.revision_id,body]);
      if(pendingConfirmation.current?.key!==key) pendingConfirmation.current={key,event:crypto.randomUUID()};
      confirm.mutate({id:view.interpretation_id,revision:current.revision_id,event:pendingConfirmation.current.event,body});
    } catch(e) {setError(e.message);}
  };
  return <section className="response-interpretation"><blockquote>{utterance.utterance_text}</blockquote><small>Original occurrence {utterance.utterance_id}; wording is preserved independently.</small>
    {current ? <><p><strong>Current interpretation:</strong> {current.interpretation_text}</p><p>{current.rationale}</p><small>{current.producer} · recorded by {current.recorded_by} · {current.recorded_at} · {current.meaning_confirmation}{current.meaning_is_provisional ? ' — meaning remains provisional' : ' — meaning confirmed, not truth'}</small><details><summary>Interpretation and meaning-confirmation history</summary><ul>{view.revisions.map(r=><li key={r.revision_id}>{r.revision_id === current.revision_id ? 'Current' : 'Historical'} · {r.meaning_confirmation} · {r.interpretation_text}<p>{r.rationale}</p><small>{r.producer} · {r.recorded_by} · {r.recorded_at}{r.supersedes_revision_id ? ` · supersedes ${r.supersedes_revision_id}` : ''}</small><ul>{view.confirmations.filter(c=>c.revision_id===r.revision_id).map(c=><li key={c.event_id}>{c.state} — {c.note} · {c.recorded_by} · {c.recorded_at}</li>)}</ul></li>)}</ul></details></> : <p>No derived interpretation recorded.</p>}
    <form onSubmit={revisionSubmit}><h4>{current?'Revise derived meaning':'Record derived meaning'}</h4><label>Interpretation<textarea value={draft.text} maxLength={8192} onChange={e=>setDraft({...draft,text:e.target.value})}/></label><label>Rationale<textarea value={draft.rationale} maxLength={4096} onChange={e=>setDraft({...draft,rationale:e.target.value})}/></label><button disabled={busy}>{current?'Record new revision':'Record human interpretation'}</button></form>
    {current && <form onSubmit={confirmationSubmit}><h4>Confirm or reject understanding of this exact revision</h4><p>Confirmation concerns meaning, not the truth of the utterance or interpretation. A new revision starts unconfirmed.</p><select aria-label="Meaning confirmation" value={confirmation.state} onChange={e=>setConfirmation({...confirmation,state:e.target.value})}><option value="Confirmed">Meaning understood correctly</option><option value="Rejected">Meaning misunderstood</option></select><label>Why<textarea value={confirmation.note} maxLength={2048} onChange={e=>setConfirmation({...confirmation,note:e.target.value})}/></label><button disabled={busy}>Record meaning confirmation</button></form>}
    {(error||record.error||confirm.error)&&<p className="error" role="alert">{error||record.error?.message||confirm.error?.message}</p>}
  </section>;
}
