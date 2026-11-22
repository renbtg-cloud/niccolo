import { situationTextError } from '../state/situationText.js';
import { useState } from 'react';
import { useClarificationAnswers } from '../hooks/useClarifications.js';

export function ClarificationCard({ question, draft, onChange, onSubmit, saving }) {
  const [pages,setPages]=useState([null]);
  const history=useClarificationAnswers(question.question_id,pages.at(-1));
  const validationError=situationTextError(draft);
  const answers=history.data?.answers??[];
  return <div className="clarification">
    <strong>{question.text}</strong>
    <p><small>Why asked: {question.reason} · based on frozen revision {question.input_revision}{question.input_is_current?'':' · Situation has changed since then'}</small></p>
    {history.isPending && <p role="status">Loading recorded answers…</p>}

    {history.isError && <div>
      <p role="alert">Recorded answers could not be loaded for this question.</p>
      <button type="button" disabled={history.isFetching} onClick={()=>void history.refetch()}>Retry loading answers</button>
    </div>}
    {history.isSuccess && answers.length===0 && <p>No answers are recorded on this page.</p>}
    {answers.length > 0 && <ul>{answers.map(answer =>
      <li key={answer.answer_id}>{answer.text}<div><small>Recorded later as {answer.kind}; not evidence and not part of the original frozen input.</small></div></li>
    )}</ul>}
    {pages.length>1 && <button type="button" disabled={history.isFetching} onClick={()=>setPages(current=>current.slice(0,-1))}>Previous answers</button>}
    {history.data?.next_after && <button type="button" disabled={history.isFetching} onClick={()=>setPages(current=>[...current,history.data.next_after])}>More answers</button>}
    <form onSubmit={event=>{event.preventDefault();if(!saving && draft.trim() && !validationError)onSubmit(draft.trim());}}>
      <label>Your answer<textarea aria-label={`Clarification answer ${question.question_id}`} value={draft} onChange={event=>onChange(event.target.value)} placeholder="Answer in your own words; this becomes attributed context."/></label>
      {validationError&&<p role="alert">{validationError}</p>}
      <button type="submit" disabled={saving||!draft.trim()||Boolean(validationError)}>{saving?'Saving…':'Add answer as context'}</button>
    </form>
  </div>;
}
