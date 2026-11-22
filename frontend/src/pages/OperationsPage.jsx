import { isCurrentSession, useSessionStore } from '../stores/session.js';
import { useRuntimeReadiness } from '../hooks/useRuntimeReadiness.js';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { situationTextError, utf8ByteLength } from '../state/situationText.js';
import { useEffect, useRef } from 'react';
import { Link } from 'react-router-dom';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { z } from 'zod';
import { qk } from '../api/keys.js';
import { useClient } from '../hooks/useClient.js';
import { useUiStore } from '../stores/ui.js';
import { useOperationObserver } from '../hooks/useOperationObserver.js';
import { useAgenticPlan } from '../hooks/useAgenticPlan.js';
import { useAnalysisOutput } from '../hooks/useAnalysisOutput.js';

const initialHumanResponse = {decision:'CONTINUE',responseText:''};

const intentSchema=z.object({
  mode:z.enum(['sync','async']),
  intent:z.enum(['ABOUT_TO_ACT_OR_SAY','UNDERSTAND','FIGURE_OUT_WHAT_MATTERS']),
  goal:z.string().superRefine((value, context)=>{
    const message=situationTextError(value);
    if(message) context.addIssue({code:'custom',message});
  }).optional(),
  stepBudget:z.coerce.number().int().min(1).max(10000),
});
const humanSchema=z.object({decision:z.enum(['CONTINUE','STOP','REPLAN']),responseText:z.string().trim().refine(text=>utf8ByteLength(text)<=4096,'Input must be at most 4096 UTF-8 bytes.').refine(text=>!text.includes('\0'),'Input cannot contain NUL.')}).refine(v=>v.decision==='STOP'||v.responseText.length>0,{path:['responseText'],message:'Enter a response to continue or replan.'});

function AnswerPanel({ output, operation, error, pending }) {
 if(error && !output)return <p className="error" role="alert">Answer unavailable: {error.message}</p>;
 if(output)return <article className="analysis-answer">
   <div className="answer-heading"><h3>Niccolò's answer</h3><span className={output.input_is_current?'status-current':'status-historical'}>{output.input_is_current?'Current input':'Historical input'}</span></div>
   {error && <p role="status">Showing the last loaded answer. Refresh failed.</p>}
   <div className="answer-text">{output.answer_text}</div>
   <h4>Coverage</h4><div className="coverage-note">{output.coverage_note}</div>
   {!output.input_is_current&&<p className="historical-note">The Situation changed after this analysis input was frozen. The original answer is preserved.</p>}
   <p><Link to="/review">Review reasoning, clarifications and next steps →</Link></p>
 </article>;
 if(pending)return <p role="status">Checking for a published answer…</p>;
 if(!operation)return <p>Start an analysis to get a provisional answer.</p>;
 if(operation.state==='WAITING_FOR_HUMAN')return <p role="status">Niccolò needs your decision before it can continue the current reasoning plan.</p>;
 if(operation.state==='FAILED')return <p className="error" role="alert">Analysis failed{operation.error_code ? ` (${operation.error_code})` : ''}. No answer was published. You can start another analysis.</p>;
 if(operation.state==='CANCELLED')return <p role="status">This analysis was cancelled. No answer was published. You can start another analysis.</p>;
 if(operation.terminal)return <p role="status">This run finished without a published synthesis. You can start another analysis.</p>;
 return <p role="status">Niccolò is working from the frozen analysis input. The answer will appear here when synthesis is published.</p>;
}

export function OperationsPage(){
 const generation=useSessionStore(s=>s.generation);
 const client=useClient(); const qc=useQueryClient();
 const readiness = useRuntimeReadiness(client);
 const modelUnavailable=readiness.data?.model_configured===false;
 const pendingLaunch=useRef(null);
 const pendingHumanResponse=useRef(null);
 const active=useUiStore(s=>s.activeOperation); const analysis=useUiStore(s=>s.analysis); const setActive=useUiStore(s=>s.setActiveOperation); const setAnalysis=useUiStore(s=>s.setAnalysis);
 const observed=useOperationObserver(active); const operation=observed.operation; const plan=useAgenticPlan(active, operation?.terminal);
 const output=useAnalysisOutput(analysis, operation?.target_analysis_id===analysis ? operation : null);
 const {register,handleSubmit,formState:{errors}}=useForm({resolver:zodResolver(intentSchema),defaultValues:{mode:'sync',intent:'UNDERSTAND',goal:'',stepBudget:32}});
 const humanForm=useForm({resolver:zodResolver(humanSchema),defaultValues:initialHumanResponse});
 const run=useMutation({mutationFn:async(v)=>{
   const signature=JSON.stringify({mode:v.mode,intent:v.intent,goal:v.goal?.trim()||null,stepBudget:v.stepBudget});
   if(pendingLaunch.current?.signature!==signature){
     pendingLaunch.current={signature,operationId:crypto.randomUUID(),analysisId:crypto.randomUUID()};
   }
   const {operationId,analysisId}=pendingLaunch.current;
   const task={analysis_id:analysisId,intent:v.intent,goal:v.goal?.trim()||null,reanalysis_of:null,idempotency_key:`ui-intent-${operationId}`,step_budget:v.stepBudget,context:{surface:'react-v1',human_owns_project:true}};
   const r=v.mode==='sync'?await client.runIntentSync(operationId,task):await client.runIntentAsync(operationId,task);
   if (isCurrentSession(generation)) {
     setAnalysis(r.value.analysis_id);
     setActive(operationId);
   }
   return r.value;
 },onSuccess:()=>{
   if (!isCurrentSession(generation)) return;
   pendingLaunch.current=null;
   void qc.invalidateQueries({queryKey:qk.analysisHistory(client.config)});
 }});
 const waitingStep=operation?.terminal ? null : plan.data?.steps?.find?.(s=>s.state==='WAITING_FOR_HUMAN');
 const waitingStepId=waitingStep?.step_id;
 const planRevision=plan.data?.plan_revision;
 const resetHumanResponse=humanForm.reset;
 useEffect(()=>{
   pendingHumanResponse.current=null;
   resetHumanResponse(initialHumanResponse);
 },[active,waitingStepId,planRevision,resetHumanResponse]);
 const respond=useMutation({mutationFn:async(v)=>{
   if(!active||!waitingStep||operation?.terminal)throw new Error('No waiting human step');
   const signature=JSON.stringify({active,step:waitingStep.step_id,decision:v.decision,text:v.responseText});
   if(pendingHumanResponse.current?.signature!==signature){
     pendingHumanResponse.current={signature,responseId:crypto.randomUUID()};
   }
   return (await client.respondAgentic(active,waitingStep.step_id,{response_id:pendingHumanResponse.current.responseId,decision:v.decision,response_text:v.responseText})).value;
 },onSuccess:async()=>{if(!isCurrentSession(generation))return;pendingHumanResponse.current=null;resetHumanResponse(initialHumanResponse);await Promise.all([qc.invalidateQueries({queryKey:['agentic-plan']}),qc.invalidateQueries({queryKey:qk.operationObservation(client.config,active),exact:true}),qc.invalidateQueries({queryKey:['operation']}),qc.invalidateQueries({queryKey:['dashboard']}),qc.invalidateQueries({queryKey:['analysis']})]);}});
 if(!client)return <p>Connect first.</p>;
 return <section><h2>Analyze this Situation</h2><p>Ask Niccolò to reason over the current Situation. The default path waits for the answer; execution details are optional.</p>
 {modelUnavailable&&<p className="error" role="alert">A reasoning model has not been configured. Configure a local or remote OpenAI-compatible model and restart the Niccolò service before analyzing.</p>}
 {readiness.isPending&&<p role="status">Checking whether a reasoning model is ready…</p>}
 {readiness.error&&<div>
   <p className="error" role="alert">Niccolò is not ready to analyze: {readiness.error.message}</p>
   <button type="button" disabled={readiness.isFetching} onClick={()=>void readiness.refetch()}>Retry readiness</button>
 </div>}
 <form onSubmit={handleSubmit(v=>{if (!run.isPending && !modelUnavailable && !readiness.isPending && !readiness.isError) run.mutate(v);})}>
   <label>What are you trying to do?<select disabled={run.isPending} {...register('intent')}><option value="ABOUT_TO_ACT_OR_SAY">I am about to say or do something</option><option value="UNDERSTAND">I am trying to understand something</option><option value="FIGURE_OUT_WHAT_MATTERS">Help me figure out what matters</option></select></label>
   <label>Goal / question (optional)<textarea disabled={run.isPending} aria-invalid={Boolean(errors.goal)} {...register('goal')} placeholder="Use your own words. Niccolò can work from the Situation even if you leave this blank."/></label>
   {errors.goal&&<p role="alert">{errors.goal.message}</p>}
   <details><summary>Advanced execution</summary><label>Delivery<select disabled={run.isPending} {...register('mode')}><option value="sync">Wait for the answer</option><option value="async">Start and follow progress</option></select></label><label>Reasoning step budget<input aria-label="Step budget" type="number" disabled={run.isPending} {...register('stepBudget')}/></label><small>The default settings are appropriate for ordinary use. Change these only when testing execution behavior.</small></details>
   <button disabled={run.isPending||modelUnavailable||readiness.isPending||readiness.isError}>{run.isPending?'Analyzing…':'Analyze this Situation'}</button>
 </form>{Object.keys(errors).length>0&&<p className="error">Check the request fields.</p>}{run.error&&<p className="error" role="alert">{run.error.message}</p>}
 {observed.isError && <div>
   <p role="alert">Could not refresh analysis progress. The last known progress may be out of date.</p>
   <button type="button" disabled={observed.isFetching} onClick={()=>void observed.refetch()}>Retry progress</button>
 </div>}
 <AnswerPanel output={output.data} operation={[operation, run.data?.operation].find(item => item?.target_analysis_id === analysis) ?? null} error={output.error} pending={Boolean(analysis) && output.isPending}/>
 {operation?.state==='WAITING_FOR_HUMAN' && !waitingStep && plan.isError && <div>
   <p role="alert">Niccolò is waiting for input. The question could not be loaded.</p>
   <button type="button" disabled={plan.isFetching} onClick={()=>void plan.refetch()}>Retry loading the question</button>
 </div>}
 {waitingStep&&<form className="human-boundary" onSubmit={humanForm.handleSubmit(v=>{if (!respond.isPending) respond.mutate(v);})}><h3>Niccolò needs your input</h3><p>{waitingStep.label}</p><p><small>Reasoning pauses here until you respond. Your input is recorded; it is not inferred consent for any external action.</small></p><label>What should happen next?<select disabled={respond.isPending} {...humanForm.register('decision')}><option value="CONTINUE">Continue with this understanding</option><option value="REPLAN">Reconsider the reasoning plan</option><option value="STOP">Stop this analysis</option></select></label><label>Your input<textarea aria-label="Human response" disabled={respond.isPending} {...humanForm.register('responseText')} placeholder="Clarify, correct, or explain what you want Niccolò to consider."/></label><button disabled={respond.isPending}>{respond.isPending?'Sending…':'Send input'}</button>{humanForm.formState.errors.responseText&&<p className="error" role="alert">{humanForm.formState.errors.responseText.message}</p>}{respond.error&&<p className="error" role="alert">{respond.error.message}</p>}</form>}
 <details className="desktop-only technical-details"><summary>Technical execution details</summary><div className="inspector-grid"><article><h3>Operation</h3><p><small>Transport: {observed.phase}{observed.stale?' · stale':''}</small></p><pre>{JSON.stringify(observed.operation??run.data?.operation??null,null,2)}</pre></article><article><h3>Agentic plan</h3><pre>{JSON.stringify(plan.data??null,null,2)}</pre></article><article><h3>Operation events</h3>{observed.eventsTruncated&&<p><small>Showing the most recent 500 events. Earlier events remain in the server history.</small></p>}<pre>{JSON.stringify(observed.events??[],null,2)}</pre></article></div></details>
 </section>;
}
