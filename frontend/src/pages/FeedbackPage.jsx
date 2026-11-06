import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useClient } from '../hooks/useClient.js';
import { useQuestions } from '../hooks/useQuestions.js';

const RESPONSE_COPY = {
  rejected: ['I reject this', 'I think the premise or interpretation is wrong.'],
  insufficient_evidence: ['Not enough evidence', 'I am not saying it is wrong; I do not think the current evidence supports it.'],
  unknown: ["I don't know", 'I cannot currently judge this one way or the other.'],
};

function QuestionCard({ question }) {
  const client = useClient();
  const qc = useQueryClient();
  const [response,setResponse] = useState('insufficient_evidence');
  const [reason,setReason] = useState('');
  const history = useQuery({
    queryKey:['question-feedback',client.config.workspace,question.question_id],
    queryFn:async()=> (await client.questionFeedback(question.question_id)).value,
  });
  const positions = useQuery({
    queryKey:['question-positions',client.config.workspace,question.question_id],
    queryFn:async()=> (await client.questionPositions(question.question_id)).value,
  });
  const submit = useMutation({
    mutationFn:()=>client.recordQuestionFeedback(question.question_id,crypto.randomUUID(),response,reason.trim()),
    onSuccess:async()=>{setReason('');await Promise.all([
      qc.invalidateQueries({queryKey:['question-feedback',client.config.workspace,question.question_id]}),
      qc.invalidateQueries({queryKey:['question-positions',client.config.workspace,question.question_id]}),
    ]);},
  });
  const latest=positions.data?.positions??[];
  return <article className="feedback-question">
    <h3>{question.text}</h3>
    <p><small>Investigative question · not evidence</small></p>
    <label>Your position<select value={response} onChange={e=>setResponse(e.target.value)}>
      {Object.entries(RESPONSE_COPY).map(([value,[label]])=><option key={value} value={value}>{label}</option>)}
    </select></label>
    <p><small>{RESPONSE_COPY[response][1]}</small></p>
    <label>Why?<textarea aria-label={`Feedback reason ${question.question_id}`} value={reason} onChange={e=>setReason(e.target.value)} placeholder="Explain your judgment in your own words"/></label>
    <button disabled={!reason.trim()||submit.isPending} onClick={()=>submit.mutate()}>{submit.isPending?'Recording…':'Record my position'}</button>
    {submit.error&&<p className="error">{submit.error.message}</p>}
    <div className="feedback-history">
      <h4>Current recorded positions</h4>
      {latest.length? <ul>{latest.map(item=><li key={item.actor_principal_id}><strong>{item.response.replaceAll('_',' ')}</strong>: {item.human_reason}</li>)}</ul> : <p>No human position recorded yet.</p>}
      <details><summary>Feedback event history</summary><ul>{(history.data?.events??[]).map(item=><li key={item.event_id}>#{item.event_no} · {item.response.replaceAll('_',' ')} · {item.human_reason}</li>)}</ul></details>
      <small>Different people and later corrections remain separate events; this does not establish consensus or truth.</small>
    </div>
  </article>;
}

export function FeedbackPage(){
  const client=useClient();
  const qc=useQueryClient();
  const questions=useQuestions();
  const [text,setText]=useState('');
  const create=useMutation({
    mutationFn:()=>client.createQuestion(crypto.randomUUID(),text.trim()),
    onSuccess:async()=>{setText('');await qc.invalidateQueries({queryKey:['questions']});},
  });
  if(!client)return <p>Connect first.</p>;
  return <section><h2>Questions & human feedback</h2>
    <p>Use different responses for different epistemic states. Rejecting an interpretation is not the same as saying the evidence is insufficient, and neither is the same as not knowing.</p>
    <article className="question-create"><h3>Add an investigative question</h3><label>Question<textarea value={text} onChange={e=>setText(e.target.value)} placeholder="What should we explicitly keep open for human judgment?"/></label><button disabled={!text.trim()||create.isPending} onClick={()=>create.mutate()}>{create.isPending?'Adding…':'Add question'}</button>{create.error&&<p className="error">{create.error.message}</p>}</article>
    {questions.isLoading&&<p>Loading questions…</p>}
    {questions.error&&<p className="error">{questions.error.message}</p>}
    <div className="feedback-list">{(questions.data?.questions??[]).map(q=><QuestionCard key={q.question_id} question={q}/>)}</div>
    {!questions.isLoading&&(questions.data?.questions??[]).length===0&&<p>No investigative questions have been recorded yet.</p>}
  </section>;
}
