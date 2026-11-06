import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { z } from 'zod';
import { useClient } from '../hooks/useClient.js';
import { useUiStore } from '../stores/ui.js';
import { useOperationObserver } from '../hooks/useOperationObserver.js';
import { useAgenticPlan } from '../hooks/useAgenticPlan.js';

const intentSchema=z.object({
  mode:z.enum(['sync','async']),
  intent:z.enum(['ABOUT_TO_ACT_OR_SAY','UNDERSTAND','FIGURE_OUT_WHAT_MATTERS']),
  goal:z.string().max(4096).optional(),
  stepBudget:z.coerce.number().int().min(1).max(10000),
});
const humanSchema=z.object({decision:z.enum(['CONTINUE','STOP','REPLAN']),responseText:z.string().trim().min(1).max(4096)});

export function OperationsPage(){
 const client=useClient(); const qc=useQueryClient();
 const active=useUiStore(s=>s.activeOperation); const setActive=useUiStore(s=>s.setActiveOperation); const setAnalysis=useUiStore(s=>s.setAnalysis);
 const observed=useOperationObserver(active); const plan=useAgenticPlan(active);
 const {register,handleSubmit,formState:{errors}}=useForm({resolver:zodResolver(intentSchema),defaultValues:{mode:'sync',intent:'UNDERSTAND',goal:'',stepBudget:32}});
 const humanForm=useForm({resolver:zodResolver(humanSchema),defaultValues:{decision:'CONTINUE',responseText:''}});
 const run=useMutation({mutationFn:async(v)=>{
   const operationId=crypto.randomUUID(); const analysisId=crypto.randomUUID();
   const task={analysis_id:analysisId,intent:v.intent,goal:v.goal?.trim()||null,reanalysis_of:null,idempotency_key:`ui-intent-${operationId}`,step_budget:v.stepBudget,context:{surface:'react-v1',human_owns_project:true}};
   const r=v.mode==='sync'?await client.runIntentSync(operationId,task):await client.runIntentAsync(operationId,task);
   setAnalysis(r.value.analysis_id); setActive(operationId); return r.value;
 }});
 const waitingStep=plan.data?.steps?.find?.(s=>s.state==='WAITING_FOR_HUMAN');
 const respond=useMutation({mutationFn:async(v)=>{if(!active||!waitingStep)throw new Error('No waiting human step');return (await client.respondAgentic(active,waitingStep.step_id,{response_id:crypto.randomUUID(),decision:v.decision,response_text:v.responseText})).value;},onSuccess:async()=>{humanForm.reset({decision:'CONTINUE',responseText:''});await Promise.all([qc.invalidateQueries({queryKey:['agentic-plan']}),qc.invalidateQueries({queryKey:['operation']}),qc.invalidateQueries({queryKey:['dashboard']})]);}});
 if(!client)return <p>Connect first.</p>;
 return <section><h2>Niccolò</h2><form onSubmit={handleSubmit(v=>run.mutate(v))}>
   <label>What are you trying to do?<select {...register('intent')}><option value="ABOUT_TO_ACT_OR_SAY">I am about to say or do something</option><option value="UNDERSTAND">I am trying to understand something</option><option value="FIGURE_OUT_WHAT_MATTERS">Help me figure out what matters</option></select></label>
   <label>Goal / question (optional)<textarea {...register('goal')} placeholder="Use your own words. Niccolò can work from the Situation even if you leave this blank."/></label>
   <label>Delivery<select {...register('mode')}><option value="sync">Wait for the server response</option><option value="async">Return immediately and observe by PULL</option></select></label>
   <label>Reasoning budget<input aria-label="Step budget" type="number" {...register('stepBudget')}/></label>
   <button disabled={run.isPending}>Start</button>
 </form>{Object.keys(errors).length>0&&<p className="error">Check the request fields.</p>}{run.error&&<p className="error">{run.error.message}</p>}
 <h3>Current run</h3><p><small>Transport: {observed.phase}{observed.stale?' · stale':''}</small></p><pre>{JSON.stringify(observed.operation??run.data?.operation??null,null,2)}</pre>
 {waitingStep&&<form className="human-boundary" onSubmit={humanForm.handleSubmit(v=>respond.mutate(v))}><h3>Human boundary</h3><p>{waitingStep.label}</p><select {...humanForm.register('decision')}><option value="CONTINUE">Continue provisionally</option><option value="REPLAN">Replan from current state</option><option value="STOP">Stop</option></select><textarea aria-label="Human response" {...humanForm.register('responseText')} placeholder="Your decision / correction / context"/><button disabled={respond.isPending}>Submit human decision</button>{humanForm.formState.errors.responseText&&<p className="error">Response is required.</p>}{respond.error&&<p className="error">{respond.error.message}</p>}</form>}
 <div className="desktop-only inspector-grid"><article><h3>Agentic plan</h3><pre>{JSON.stringify(plan.data??null,null,2)}</pre></article><article><h3>Operation events</h3><pre>{JSON.stringify(observed.events??[],null,2)}</pre></article></div>
 <small>V1 uses ordinary HTTP request/response and PULL. Sync and async are delivery choices over the same server-side intent semantics.</small></section>;
}
