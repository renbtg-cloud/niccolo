import {useRef,useState} from 'react';

export function decisionBasisReviewRequest(change,disposition,note) {
  if (!change.original_assessment_event_id || !change.current_assessment_event_id || change.original_assessment_event_id===change.current_assessment_event_id) throw new Error('A changed assessment basis is required.');
  if (!['RETAINED','RECONSIDERATION_REQUIRED','DEFERRED'].includes(disposition) || !note.trim() || note.trim().length>4096) throw new Error('Choose a disposition and give a review note.');
  return {original_assessment_event_id:change.original_assessment_event_id,reviewed_assessment_event_id:change.current_assessment_event_id,disposition,note:note.trim()};
}

export function DecisionBasisReviewPanel({change,assessments,client,onChanged}) {
  const [disposition,setDisposition]=useState('');
  const [note,setNote]=useState('');
  const [saving,setSaving]=useState(false);
  const [error,setError]=useState('');
  const pending=useRef(null);
  const original=assessments.get(change.original_assessment_event_id);
  const current=assessments.get(change.current_assessment_event_id);
  async function save(e) {
    e.preventDefault();if(saving)return;
    try {
      const body=decisionBasisReviewRequest(change,disposition,note);
      const key=JSON.stringify(body);
      if(pending.current?.key!==key)pending.current={key,id:crypto.randomUUID()};
      setSaving(true);setError('');
      await client.recordDecisionBasisReview(change.decision_id,pending.current.id,body);
      pending.current=null;setDisposition('');setNote('');await onChanged();
    } catch(e){setError(e.message);}finally{setSaving(false);}
  }
  const describe=(a,id)=>a?`${a.input.dimension}: ${a.input.result} — ${a.input.rationale} (${id}; by ${a.recorded_by})`:`Assessment unavailable (${id})`;
  return <section><h4>{change.requires_review?'Changed basis: review attention':'Current changed basis explicitly retained'}</h4>
    <p>Original: {describe(original,change.original_assessment_event_id)}</p>
    <p>Current: {describe(current,change.current_assessment_event_id)}</p>
    <p>Review applies only to this exact change. It does not rewrite or execute the recorded decision.</p>
    {change.review_history.length>0&&<ul>{change.review_history.map(r=><li key={r.review_event_id}>{r.input.disposition}: {r.input.note} · {r.input.reviewed_assessment_event_id} · by {r.recorded_by} · {r.recorded_at}</li>)}</ul>}
    <form onSubmit={save}><fieldset disabled={saving||!original||!current}><legend>Review decision basis</legend><label>Disposition<select required value={disposition} onChange={e=>setDisposition(e.target.value)}><option value="">Choose…</option><option value="RETAINED">Retain decision after review</option><option value="RECONSIDERATION_REQUIRED">Reconsideration required</option><option value="DEFERRED">Defer review</option></select></label><label>Review note<textarea required maxLength={4096} value={note} onChange={e=>setNote(e.target.value)}/></label><button type="submit">Record review only</button>{error&&<p role="alert">{error}</p>}</fieldset></form>
  </section>;
}
