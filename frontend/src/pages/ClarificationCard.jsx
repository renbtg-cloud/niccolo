export function ClarificationCard({ question, draft, onChange, onSubmit, saving }) {
  return <div className="clarification">
    <strong>{question.text}</strong>
    <p><small>Why asked: {question.reason} · based on frozen revision {question.input_revision}{question.input_is_current?'':' · Situation has changed since then'}</small></p>
    {question.moreAnswers && <p>Showing the first page of recorded answers.</p>}
    {question.answersUnavailable && <p role="status">Earlier answers could not be loaded for this question.</p>}
    {question.answers?.length > 0 && <ul>{question.answers.map(answer =>
      <li key={answer.answer_id}>{answer.text}<div><small>Recorded later as {answer.kind}; not evidence and not part of the original frozen input.</small></div></li>
    )}</ul>}
    <label>Your answer<textarea aria-label={`Clarification answer ${question.question_id}`} value={draft} onChange={event=>onChange(event.target.value)} placeholder="Answer in your own words; this becomes attributed context."/></label>
    <button disabled={saving||!draft.trim()} onClick={()=>onSubmit(draft.trim())}>Add answer as context</button>
  </div>;
}
