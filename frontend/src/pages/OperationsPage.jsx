import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useRef } from 'react';
import { Link } from 'react-router-dom';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { z } from 'zod';
import { useClient } from '../hooks/useClient.js';
import { useUiStore } from '../stores/ui.js';
import { useOperationObserver } from '../hooks/useOperationObserver.js';
import { useAgenticPlan } from '../hooks/useAgenticPlan.js';
import { useAnalysisOutput } from '../hooks/useAnalysisOutput.js';

const intentSchema=z.object({
  mode:z.enum(['sync','async']),
  intent:z.enum(['ABOUT_TO_ACT_OR_SAY','UNDERSTAND','FIGURE_OUT_WHAT_MATTERS']),
  goal:z.string().max(4096).optional(),
  stepBudget:z.coerce.number().int().min(1).max(10000),
});
const humanSchema=z.object({decision:z.enum(['CONTINUE','STOP','REPLAN']),responseText:z.string().trim().max(4096)}).refine(v=>v.decision==='STOP'||v.responseText.length>0,{path:['responseText']});

function AnswerPanel({ output, operation, error }) {
 if(error)return <p className="error">Answer unavailable: {error.message}</p>;
 if(output)return <article className="analysis-answer">
   <div className="answer-heading"><h3>Niccolò's answer</h3><span className={output.input_is_current?'status-current':'status-historical'}>{output.input_is_current?'Current input':'Historical input'}</span></div>
   <div className="answer-text">{output.answer_text}</div>
   <h4>Coverage</h4><div className="coverage-note">{output.coverage_note}</div>
   {!output.input_is_current&&<p className="historical-note">The Situation changed after this analysis input was frozen. The original answer is preserved.</p>}
   <p><Link to="/review">Review reasoning, clarifications and next steps →</Link></p>
 </article>;
 if(!operation)return <p>Start an analysis to get a provisional answer.</p>;
 if(operation.state==='WAITING_FOR_HUMAN')return <p>Niccolò needs your decision before it can continue the current reasoning plan.</p>;
 if(operation.state==='FAILED')return <p className="error" role="alert">Analysis failed{operation.error_code ? ` (${operation.error_code})` : ''}. No answer was published. You can start another analysis.</p>;
 if(operation.state==='CANCELLED')return <p role="status">This analysis was cancelled. No answer was published. You can start another analysis.</p>;
 if(operation.terminal)return <p role="status">This run finished without a published synthesis. You can start another analysis.</p>;
 return <p>Niccolò is working from the frozen analysis input. The answer will appear here when synthesis is published.</p>;
}

export function OperationsPage(){
 const client=useClient(); const qc=useQueryClient();
 const pendingLaunch=useRef(null);
 const pendingHumanResponse=useRef(null);
 const active=useUiStore(s=>s.activeOperation); const analysis=useUiStore(s=>s.analysis); const setActive=useUiStore(s=>s.setActiveOperation); const setAnalysis=useUiStore(s=>s.setAnalysis);
 const observed=useOperationObserver(active); const plan=useAgenticPlan(active); const output=useAnalysisOutput(analysis,observed.operation);
 const {register,handleSubmit,formState:{errors}}=useForm({resolver:zodResolver(intentSchema),defaultValues:{mode:'sync',intent:'UNDERSTAND',goal:'',stepBudget:32}});
 const humanForm=useForm({resolver:zodResolver(humanSchema),defaultValues:{decision:'CONTINUE',responseText:''}});
 const run=useMutation({mutationFn:async(v)=>{
   const signature=JSON.stringify({mode:v.mode,intent:v.intent,goal:v.goal?.trim()||null,stepBudget:v.stepBudget});
   if(pendingLaunch.current?.signature!==signature){
     pendingLaunch.current={signature,operationId:crypto.randomUUID(),analysisId:crypto.randomUUID()};
   }
   const {operationId,analysisId}=pendingLaunch.current;
   const task={analysis_id:analysisId,intent:v.intent,goal:v.goal?.trim()||null,reanalysis_of:null,idempotency_key:`ui-intent-${operationId}`,step_budget:v.stepBudget,context:{surface:'react-v1',human_owns_project:true}};
   const r=v.mode==='sync'?await client.runIntentSync(operationId,task):await client.runIntentAsync(operationId,task);
   setAnalysis(r.value.analysis_id); setActive(operationId); return r.value;
 },onSuccess:()=>{pendingLaunch.current=null;}});
 const waitingStep=plan.data?.steps?.find?.(s=>s.state==='WAITING_FOR_HUMAN');
 const respond=useMutation({mutationFn:async(v)=>{
   if(!active||!waitingStep)throw new Error('No waiting human step');
   const signature=JSON.stringify({active,step:waitingStep.step_id,decision:v.decision,text:v.responseText});
   if(pendingHumanResponse.current?.signature!==signature){
     pendingHumanResponse.current={signature,responseId:crypto.randomUUID()};
   }
   return (await client.respondAgentic(active,waitingStep.step_id,{response_id:pendingHumanResponse.current.responseId,decision:v.decision,response_text:v.responseText})).value;
 },onSuccess:async()=>{pendingHumanResponse.current=null;humanForm.reset({decision:'CONTINUE',responseText:''});await Promise.all([qc.invalidateQueries({queryKey:['agentic-plan']}),qc.invalidateQueries({queryKey:['operation']}),qc.invalidateQueries({queryKey:['dashboard']}),qc.invalidateQueries({queryKey:['analysis']})]);}});
 if(!client)return <p>Connect first.</p>;
 return <section><h2>Analyze this Situation</h2><p>Ask Niccolò to reason over the current Situation. The default path waits for the answer; execution details are optional.</p><form onSubmit={handleSubmit(v=>run.mutate(v))}>
   <label>What are you trying to do?<select {...register('intent')}><option value="ABOUT_TO_ACT_OR_SAY">I am about to say or do something</option><option value="UNDERSTAND">I am trying to understand something</option><option value="FIGURE_OUT_WHAT_MATTERS">Help me figure out what matters</option></select></label>
   <label>Goal / question (optional)<textarea {...register('goal')} placeholder="Use your own words. Niccolò can work from the Situation even if you leave this blank."/></label>
   <details><summary>Advanced execution</summary><label>Delivery<select {...register('mode')}><option value="sync">Wait for the answer</option><option value="async">Return immediately and observe by PULL</option></select></label><label>Reasoning step budget<input aria-label="Step budget" type="number" {...register('stepBudget')}/></label><small>The default settings are appropriate for ordinary use. Change these only when testing execution behavior.</small></details>
   <button disabled={run.isPending}>{run.isPending?'Analyzing…':'Analyze this Situation'}</button>
 </form>{Object.keys(errors).length>0&&<p className="error">Check the request fields.</p>}{run.error&&<p className="error">{run.error.message}</p>}
 <AnswerPanel output={output.data} operation={observed.operation??run.data?.operation??null} error={output.error}/>
 {waitingStep&&<form className="human-boundary" onSubmit={humanForm.handleSubmit(v=>respond.mutate(v))}><h3>Niccolò needs your input</h3><p>{waitingStep.label}</p><p><small>Reasoning pauses here until you respond. Your input is recorded; it is not inferred consent for any external action.</small></p><label>What should happen next?<select {...humanForm.register('decision')}><option value="CONTINUE">Continue with this understanding</option><option value="REPLAN">Reconsider the reasoning plan</option><option value="STOP">Stop this analysis</option></select></label><label>Your input<textarea aria-label="Human response" {...humanForm.register('responseText')} placeholder="Clarify, correct, or explain what you want Niccolò to consider."/></label><button disabled={respond.isPending}>{respond.isPending?'Sending…':'Send input'}</button>{humanForm.formState.errors.responseText&&<p className="error">A response is required to continue or replan, but not to stop.</p>}{respond.error&&<p className="error">{respond.error.message}</p>}</form>}
 <details className="desktop-only technical-details"><summary>Technical execution details</summary><div className="inspector-grid"><article><h3>Operation</h3><p><small>Transport: {observed.phase}{observed.stale?' · stale':''}</small></p><pre>{JSON.stringify(observed.operation??run.data?.operation??null,null,2)}</pre></article><article><h3>Agentic plan</h3><pre>{JSON.stringify(plan.data??null,null,2)}</pre></article><article><h3>Operation events</h3>{observed.eventsTruncated&&<p><small>Showing the most recent 500 events. Earlier events remain in the server history.</small></p>}<pre>{JSON.stringify(observed.events??[],null,2)}</pre></article></div></details>
 <small>Advanced delivery choices use the same durable server-side intent semantics. They do not change the meaning of the analysis.</small></section>;
}
