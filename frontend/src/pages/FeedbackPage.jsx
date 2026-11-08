import { useRef, useState } from 'react';
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
  const [eventCursor,setEventCursor] = useState(0);
  const [eventBackStack,setEventBackStack] = useState([]);
  const pendingFeedback = useRef(null);
  const history = useQuery({
    queryKey:['question-feedback',client.config.workspace,question.question_id,eventCursor],
    queryFn:async()=> (await client.questionFeedback(question.question_id,eventCursor,100)).value,
  });
  const positions = useQuery({
    queryKey:['question-positions',client.config.workspace,question.question_id],
    queryFn:async()=> (await client.questionPositions(question.question_id)).value,
  });
  const submit = useMutation({
    mutationFn:()=>{
      const recordedReason = reason.trim();
      const signature = JSON.stringify({ questionId: question.question_id, response, reason: recordedReason });
      if (pendingFeedback.current?.signature !== signature) {
        pendingFeedback.current = { signature, eventId: crypto.randomUUID() };
      }
      return client.recordQuestionFeedback(question.question_id, pendingFeedback.current.eventId, response, recordedReason);
    },
    onSuccess:async()=>{pendingFeedback.current=null;setReason('');await Promise.all([
      qc.invalidateQueries({queryKey:['question-feedback',client.config.workspace,question.question_id]}),
      qc.invalidateQueries({queryKey:['question-positions',client.config.workspace,question.question_id]}),
    ]);},
  });
  const latest=positions.data?.positions??[];
  return <article className="feedback-question">
    <h3>{question.text}</h3>
    <p><small>Investigative question · not evidence</small></p>
    <label>Your position<select value={response} disabled={submit.isPending} onChange={e=>{submit.reset();setResponse(e.target.value);}}>
      {Object.entries(RESPONSE_COPY).map(([value,[label]])=><option key={value} value={value}>{label}</option>)}
    </select></label>
    <p><small>{RESPONSE_COPY[response][1]}</small></p>
    <label>Why?<textarea aria-label={`Feedback reason ${question.question_id}`} value={reason} disabled={submit.isPending} onChange={e=>{submit.reset();setReason(e.target.value);}} placeholder="Explain your judgment in your own words"/></label>
    <button disabled={!reason.trim()||submit.isPending} onClick={()=>submit.mutate()}>{submit.isPending?'Recording…':'Record my position'}</button>
    {submit.error&&<p className="error">{submit.error.message}</p>}
    {submit.isSuccess&&<p role="status">Your position was recorded.</p>}
    <div className="feedback-history">
      <h4>Current recorded positions</h4>
      {latest.length? <ul>{latest.map(item=><li key={item.actor_principal_id}><strong>{item.response.replaceAll('_',' ')}</strong>: {item.human_reason}</li>)}</ul> : <p>No human position recorded yet.</p>}
      <details><summary>Feedback event history</summary>
        {history.isLoading&&<p>Loading feedback events…</p>}
        {history.error&&<p className="error">{history.error.message}</p>}
        <ul>{(history.data?.events??[]).map(item=><li key={item.event_id}>#{item.event_no} · {item.response.replaceAll('_',' ')} · {item.human_reason}</li>)}</ul>
        <div className="row">
          <button disabled={history.isFetching||eventBackStack.length===0} onClick={()=>{
            const stack=eventBackStack.slice();
            setEventCursor(stack.pop()??0);
            setEventBackStack(stack);
          }}>Previous feedback events</button>
          <button disabled={history.isFetching||!history.data?.next_after} onClick={()=>{
            setEventBackStack(stack=>[...stack,eventCursor]);
            setEventCursor(history.data.next_after);
          }}>Next feedback events</button>
        </div>
      </details>
      <small>Different people and later corrections remain separate events; this does not establish consensus or truth.</small>
    </div>
  </article>;
}

export function FeedbackPage(){
  const client=useClient();
  const qc=useQueryClient();
  const [questionCursor,setQuestionCursor]=useState(null);
  const [questionBackStack,setQuestionBackStack]=useState([]);
  const questions=useQuestions(questionCursor);
  const [text,setText]=useState('');
  const pendingQuestion = useRef(null);
  const create=useMutation({
    mutationFn:()=>{
      const question = text.trim();
      if (pendingQuestion.current?.question !== question) {
        pendingQuestion.current = { question, questionId: crypto.randomUUID() };
      }
      return client.createQuestion(pendingQuestion.current.questionId, question);
    },
    onSuccess:async()=>{pendingQuestion.current=null;setText('');await qc.invalidateQueries({queryKey:['questions']});},
  });
  if(!client)return <p>Connect first.</p>;
  return <section><h2>Questions & human feedback</h2>
    <p>Use different responses for different epistemic states. Rejecting an interpretation is not the same as saying the evidence is insufficient, and neither is the same as not knowing.</p>
    <article className="question-create"><h3>Add an investigative question</h3><label>Question<textarea value={text} disabled={create.isPending} onChange={e=>{create.reset();setText(e.target.value);}} placeholder="What should we explicitly keep open for human judgment?"/></label><button disabled={!text.trim()||create.isPending} onClick={()=>create.mutate()}>{create.isPending?'Adding…':'Add question'}</button>{create.error&&<p className="error">{create.error.message}</p>}{create.isSuccess&&<p role="status">Investigative question recorded. Use question navigation to browse older records.</p>}</article>
    {questions.isLoading&&<p>Loading questions…</p>}
    {questions.error&&<p className="error">{questions.error.message}</p>}
    <div className="feedback-list">{(questions.data?.questions??[]).map(q=><QuestionCard key={q.question_id} question={q}/>)}</div>
    {!questions.isLoading&&(questions.data?.questions??[]).length===0&&<p>No investigative questions on this page.</p>}
    <div className="row">
      <button disabled={questions.isFetching||questionBackStack.length===0} onClick={()=>{
        const stack=questionBackStack.slice();
        setQuestionCursor(stack.pop()??null);
        setQuestionBackStack(stack);
      }}>Previous questions</button>
      <button disabled={questions.isFetching||!questions.data?.next_after} onClick={()=>{
        setQuestionBackStack(stack=>[...stack,questionCursor]);
        setQuestionCursor(questions.data.next_after);
      }}>Next questions</button>
    </div>
  </section>;
}
