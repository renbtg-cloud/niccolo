import { useState } from 'react';
import { useClarificationAnswers } from '../hooks/useClarifications.js';

export function ClarificationCard({ question, draft, onChange, onSubmit, saving }) {
  const [pages,setPages]=useState([null]);
  const history=useClarificationAnswers(question.question_id,pages.at(-1));
  const answers=history.data?.answers??[];
  return <div className="clarification">
    <strong>{question.text}</strong>
    <p><small>Why asked: {question.reason} · based on frozen revision {question.input_revision}{question.input_is_current?'':' · Situation has changed since then'}</small></p>
    {history.isPending && <p role="status">Loading recorded answers…</p>}

    {history.isError && <p role="status">Earlier answers could not be loaded for this question.</p>}
    {answers.length > 0 && <ul>{answers.map(answer =>
      <li key={answer.answer_id}>{answer.text}<div><small>Recorded later as {answer.kind}; not evidence and not part of the original frozen input.</small></div></li>
    )}</ul>}
    {pages.length>1 && <button type="button" onClick={()=>setPages(current=>current.slice(0,-1))}>Previous answers</button>}
    {history.data?.next_after && <button type="button" onClick={()=>setPages(current=>[...current,history.data.next_after])}>More answers</button>}
    <label>Your answer<textarea aria-label={`Clarification answer ${question.question_id}`} value={draft} onChange={event=>onChange(event.target.value)} placeholder="Answer in your own words; this becomes attributed context."/></label>
    <button disabled={saving||!draft.trim()} onClick={()=>onSubmit(draft.trim())}>Add answer as context</button>
  </div>;
}
