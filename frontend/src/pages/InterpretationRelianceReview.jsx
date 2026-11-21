import { useRef, useState } from 'react';
import { useMutation } from '@tanstack/react-query';
import { interpretationReviewRequest, reassessmentDispositions } from '../state/expertWorkbench.js';

export function InterpretationRelianceReview({view,route,reviewView,client,onChanged}) {
  const [draft,setDraft]=useState({disposition:'DEFERRED',note:''});
  const [error,setError]=useState('');
  const pending=useRef(null);
  const mutation=useMutation({mutationFn:({event,body})=>client.reviewInterpretationDependency(view.dependency.dependency_id,event,body),onSuccess:async()=>{pending.current=null;setDraft({disposition:'DEFERRED',note:''});await onChanged();}});
  const submit=e=>{e.preventDefault();setError('');try{
    const body=interpretationReviewRequest(view,route,draft.disposition,draft.note);
    const key=JSON.stringify(body);
    if(pending.current?.key!==key)pending.current={key,event:crypto.randomUUID()};
    mutation.mutate({event:pending.current.event,body});
  }catch(e){setError(e.message)}};
  return <div><p>{reviewView.requires_review?'Review required':'Retained after review'}</p>{reviewView.latest_review&&<p><strong>{reviewView.latest_review.input.disposition}</strong> — {reviewView.latest_review.input.note}</p>}<form onSubmit={submit}><label>Review disposition<select value={draft.disposition} onChange={e=>setDraft({...draft,disposition:e.target.value})}>{reassessmentDispositions.map(x=><option key={x}>{x}</option>)}</select></label><label>Review note<input maxLength={4096} value={draft.note} onChange={e=>setDraft({...draft,note:e.target.value})}/></label><button disabled={mutation.isPending}>Record interpretation review</button></form>{(error||mutation.error)&&<p role="alert" className="error">{error||mutation.error.message}</p>}{reviewView.review_history.length>0&&<details><summary>Interpretation review history</summary><ul>{reviewView.review_history.map(r=><li key={r.review_event_id}>{r.input.disposition} — {r.input.note} · revision {r.input.reviewed_revision_id} · confirmation {r.input.reviewed_confirmation_event_id??'none'} · {r.recorded_by} · {r.recorded_at}</li>)}</ul></details>}</div>;
}
