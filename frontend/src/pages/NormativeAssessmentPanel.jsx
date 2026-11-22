import {useRef,useState} from 'react';

export const normativeDimensions=['APPLICABILITY','AUTHORIZATION','OBLIGATION','PROHIBITION','COMPLIANCE','VIOLATION','PRECEDENCE','EXCEPTION'];
const judgments=['INDICATED','NOT_INDICATED','UNKNOWN','CONTESTED'];
const objectKinds=['PROPOSITION','HYPOTHESIS','ASSUMPTION','FINDING'];
const comparative=dimension=>['PRECEDENCE','EXCEPTION'].includes(dimension);
const label=value=>value.replaceAll('_',' ').toLowerCase();
const empty={subject_actor_id:'',action_object_id:'',norm_object_id:'',issuer_actor_id:'',competing_norm_object_id:null,competing_issuer_actor_id:null,dimension:'',judgment:'',context:'',temporal_scope:'',rationale:'',basis_evidence_id:null,revises_event_id:null};
const identity=['subject_actor_id','action_object_id','norm_object_id','issuer_actor_id','competing_norm_object_id','competing_issuer_actor_id','dimension','context','temporal_scope'];

export function normativeRequest(draft,actors,nodes,evidence,events){
  const actor=id=>actors.some(a=>a.actor_id===id),object=id=>nodes.some(n=>n.object_id===id&&objectKinds.includes(n.kind));
  const body={...draft,competing_norm_object_id:draft.competing_norm_object_id||null,competing_issuer_actor_id:draft.competing_issuer_actor_id||null,basis_evidence_id:draft.basis_evidence_id||null,revises_event_id:draft.revises_event_id||null};
  if(!actor(body.subject_actor_id)||!actor(body.issuer_actor_id)||!object(body.action_object_id)||!object(body.norm_object_id)||body.action_object_id===body.norm_object_id)throw new Error('Select the actor, action, distinct norm and issuing authority.');
  if(!normativeDimensions.includes(body.dimension)||!judgments.includes(body.judgment))throw new Error('Select the independent dimension and judgment.');
  if(comparative(body.dimension)){
    if(!object(body.competing_norm_object_id)||!actor(body.competing_issuer_actor_id)||[body.action_object_id,body.norm_object_id].includes(body.competing_norm_object_id))throw new Error('Identify a distinct competing norm and its issuing authority.');
  }else if(body.competing_norm_object_id||body.competing_issuer_actor_id)throw new Error('Only precedence or exception comparisons allow a competing norm.');
  for(const [field,max] of [['context',2048],['temporal_scope',2048],['rationale',4096]]){
    body[field]=body[field].trim();
    if(!body[field]||new TextEncoder().encode(body[field]).length>max)throw new Error('Context, temporal scope and rationale are required within their size limits.');
  }
  if(body.basis_evidence_id&&!evidence.some(e=>e.evidence_id===body.basis_evidence_id))throw new Error('Select a visible evidence fragment.');
  if(body.revises_event_id){
    const prior=events.find(e=>e.event_id===body.revises_event_id);
    if(!prior||events.some(e=>e.input.revises_event_id===prior.event_id)||identity.some(k=>(prior.input[k]??null)!==body[k]))throw new Error('Revision must preserve the exact current norm, issuer, action, dimension and scope.');
  }
  return body;
}

export function NormativeAssessmentPanel({snapshot,actors,client,onChanged}){
  const [draft,setDraft]=useState(empty),[saving,setSaving]=useState(false),[error,setError]=useState('');
  const pending=useRef(null);
  const nodes=(snapshot.nodes??[]).filter(n=>objectKinds.includes(n.kind)),evidence=snapshot.evidence??[],events=snapshot.normative_assessments??[];
  const revised=new Set(events.map(e=>e.input.revises_event_id)),locked=!!draft.revises_event_id;
  const actorLabel=id=>actors.find(a=>a.actor_id===id)?.display_label??id;
  const objectLabel=id=>nodes.find(n=>n.object_id===id)?.text??id;
  const change=(key,value)=>setDraft(d=>({...d,[key]:value}));
  const actorSelect=(field,title)=> <label>{title}<select required disabled={locked} value={draft[field]??''} onChange={e=>change(field,e.target.value||null)}><option value="">Choose…</option>{actors.map(a=><option key={a.actor_id} value={a.actor_id}>{a.display_label}</option>)}</select></label>;
  const objectSelect=(field,title)=> <label>{title}<select required disabled={locked} value={draft[field]??''} onChange={e=>change(field,e.target.value||null)}><option value="">Choose…</option>{nodes.map(n=><option key={n.object_id} value={n.object_id}>{n.text} ({label(n.kind)})</option>)}</select></label>;
  async function save(e){e.preventDefault();if(saving)return;try{const body=normativeRequest(draft,actors,nodes,evidence,events);const key=JSON.stringify(body);if(pending.current?.key!==key)pending.current={key,id:crypto.randomUUID()};setSaving(true);setError('');await client.recordNormativeAssessment(pending.current.id,body);pending.current=null;setDraft(empty);await onChanged();}catch(e){setError(e.message);}finally{setSaving(false);}}
  return <details><summary>Conflicting norms and obligations — attributed assessments</summary>
    <p>Evaluate one specific action under one identified norm and issuing authority. Applicability, authorization, obligation, prohibition, compliance and violation remain separate judgments. Precedence and exceptions compare exact norms. Competing assessors can disagree. These records do not establish legality, loyalty or real system permissions.</p>
    <form onSubmit={save}><fieldset disabled={saving||!actors.length||nodes.length<2}><legend>{locked?'Revise exact normative assessment':'Record independent normative assessment'}</legend>
      {actorSelect('subject_actor_id','Actor whose action is assessed')}
      {objectSelect('action_object_id','Exact action statement')}
      {objectSelect('norm_object_id','Exact norm or obligation statement')}
      {actorSelect('issuer_actor_id','Attributed norm issuer')}
      <label>Dimension<select required disabled={locked} value={draft.dimension} onChange={e=>setDraft(d=>({...d,dimension:e.target.value,competing_norm_object_id:null,competing_issuer_actor_id:null,judgment:''}))}><option value="">Choose…</option>{normativeDimensions.map(d=><option key={d} value={d}>{label(d)}</option>)}</select></label>
      {comparative(draft.dimension)&&<><p>{draft.dimension==='PRECEDENCE'?'The first norm is claimed to take precedence over the competing norm in this context.':'The first norm is claimed to provide an exception to the competing norm in this context.'}</p>{objectSelect('competing_norm_object_id','Exact competing norm statement')}{actorSelect('competing_issuer_actor_id','Attributed competing norm issuer')}</>}
      <label>Judgment<select required value={draft.judgment} onChange={e=>change('judgment',e.target.value)}><option value="">Choose…</option>{judgments.map(j=><option key={j} value={j}>{label(j)}</option>)}</select></label>
      <label>Context and scope of applicability<input required disabled={locked} maxLength={2048} value={draft.context} onChange={e=>change('context',e.target.value)}/></label>
      <label>Temporal scope<input required disabled={locked} maxLength={2048} value={draft.temporal_scope} onChange={e=>change('temporal_scope',e.target.value)}/></label>
      <label>Assessor interpretation and rationale<textarea required maxLength={4096} value={draft.rationale} onChange={e=>change('rationale',e.target.value)}/></label>
      <label>Evidence fragment<select value={draft.basis_evidence_id??''} onChange={e=>change('basis_evidence_id',e.target.value||null)}><option value="">Not explicitly linked</option>{evidence.map(e=><option key={e.evidence_id} value={e.evidence_id}>{e.speaker_label}: {e.content?.slice(0,120)} ({e.evidence_id})</option>)}</select></label>
      <button type="submit">Record normative judgment only</button>{locked&&<button type="button" onClick={()=>setDraft(empty)}>Cancel revision</button>}{error&&<p role="alert">{error}</p>}
    </fieldset></form>
    <h4>Attributed normative history</h4><p>Historical and competing records remain visible; neither creates an automatic hierarchy nor overrides another assessor.</p>
    {!events.length?<p>No explicit normative assessment is recorded.</p>:<ul>{events.map(e=><li key={e.event_id}><strong>{actorLabel(e.input.subject_actor_id)} — {label(e.input.dimension)}: {label(e.input.judgment)}</strong><p>Action: {objectLabel(e.input.action_object_id)}</p><p>Norm: {objectLabel(e.input.norm_object_id)} · attributed issuer: {actorLabel(e.input.issuer_actor_id)}</p>{e.input.competing_norm_object_id&&<p>Competing norm: {objectLabel(e.input.competing_norm_object_id)} · attributed issuer: {actorLabel(e.input.competing_issuer_actor_id)}</p>}<p>{e.input.context} · {e.input.temporal_scope}</p><p>{e.input.rationale}</p><small>By {e.recorded_by} · {e.recorded_at} · {e.event_id} · basis {e.input.basis_evidence_id??'not explicitly linked'}</small>{revised.has(e.event_id)?<p>Historical — explicitly revised.</p>:<button type="button" disabled={saving} onClick={()=>setDraft({...e.input,revises_event_id:e.event_id})}>Propose revision of this assessment</button>}</li>)}</ul>}
  </details>;
}
