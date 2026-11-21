import {useRef,useState} from 'react';
export const actKinds=['INVESTIGATING','IGNORING','DEEPENING','DEFERRING','AVOIDING_ACQUISITION','DELEGATING','RECONSIDERING','CHALLENGING','PRESERVING_AMBIGUITY'];
const characterizations=['REPORTED','OBSERVED','INFERRED','CONTESTED'];
const empty={subject_actor_id:'',target_object_id:'',act_kind:'',characterization:'',delegate_actor_id:null,context:'',temporal_scope:'',rationale:'',basis_evidence_id:null,revises_event_id:null};
const label=s=>s.replaceAll('_',' ').toLowerCase();
export function actRequest(draft,actors,nodes,evidence,events){
  const actor=id=>actors.some(a=>a.actor_id===id);
  if(!actor(draft.subject_actor_id)||!nodes.some(n=>n.object_id===draft.target_object_id))throw new Error('Select a visible actor and object.');
  if(!actKinds.includes(draft.act_kind)||!characterizations.includes(draft.characterization))throw new Error('Choose an act and its attributed characterization.');
  if(draft.delegate_actor_id&&(draft.act_kind!=='DELEGATING'||draft.delegate_actor_id===draft.subject_actor_id||!actor(draft.delegate_actor_id)))throw new Error('Delegation must identify a different visible actor, if specified.');
  for(const [field,max] of [['context',2048],['temporal_scope',2048],['rationale',4096]])if(!draft[field].trim()||draft[field].trim().length>max)throw new Error('Context, temporal scope and rationale are required.');
  if(draft.basis_evidence_id&&!evidence.some(e=>e.evidence_id===draft.basis_evidence_id)||draft.characterization==='OBSERVED'&&!draft.basis_evidence_id)throw new Error('Observed acts require visible explicit evidence.');
  const body={...draft,delegate_actor_id:draft.delegate_actor_id||null,basis_evidence_id:draft.basis_evidence_id||null,context:draft.context.trim(),temporal_scope:draft.temporal_scope.trim(),rationale:draft.rationale.trim()};
  if(body.revises_event_id){const prior=events.find(e=>e.event_id===body.revises_event_id);if(!prior||events.some(e=>e.input.revises_event_id===prior.event_id)||['subject_actor_id','target_object_id','act_kind','delegate_actor_id','context','temporal_scope'].some(k=>prior.input[k]!==body[k]))throw new Error('Revision must preserve the current attributed act identity.');}
  return body;
}
export function ActorEpistemicActPanel({snapshot,actors,client,onChanged}){
  const [draft,setDraft]=useState(empty),[saving,setSaving]=useState(false),[error,setError]=useState('');
  const pending=useRef(null),nodes=snapshot.nodes??[],evidence=snapshot.evidence??[],events=snapshot.actor_epistemic_acts??[];
  const revised=new Set(events.map(e=>e.input.revises_event_id)),actorById=new Map(actors.map(a=>[a.actor_id,a.display_label])),nodeById=new Map(nodes.map(n=>[n.object_id,n.text]));
  const change=(k,value)=>setDraft(d=>({...d,[k]:value})),locked=!!draft.revises_event_id;
  async function save(e){e.preventDefault();if(saving)return;try{const body=actRequest(draft,actors,nodes,evidence,events),key=JSON.stringify(body);if(pending.current?.key!==key)pending.current={key,id:crypto.randomUUID()};setSaving(true);setError('');await client.recordActorEpistemicAct(pending.current.id,body);pending.current=null;setDraft(empty);await onChanged();}catch(e){setError(e.message);}finally{setSaving(false);}}
  return <details><summary>Actor epistemic acts — attributed occurrence descriptions</summary><p>Describe acts, not instructions. Reporting or observing delegation does not delegate work, grant authorization, send a message, change a budget or prove a persistent disposition.</p>
    <form onSubmit={save}><fieldset disabled={saving||!actors.length||!nodes.length}><legend>{locked?'Revise exact act characterization':'Record an act description'}</legend>
      <label>Actor<select required disabled={locked} value={draft.subject_actor_id} onChange={e=>setDraft(d=>({...d,subject_actor_id:e.target.value,delegate_actor_id:null}))}><option value="">Choose…</option>{actors.map(a=><option key={a.actor_id} value={a.actor_id}>{a.display_label}</option>)}</select></label>
      <label>Object<select required disabled={locked} value={draft.target_object_id} onChange={e=>change('target_object_id',e.target.value)}><option value="">Choose…</option>{nodes.map(n=><option key={n.object_id} value={n.object_id}>{n.text}</option>)}</select></label>
      <label>Act<select required disabled={locked} value={draft.act_kind} onChange={e=>setDraft(d=>({...d,act_kind:e.target.value,delegate_actor_id:null}))}><option value="">Choose…</option>{actKinds.map(k=><option key={k} value={k}>{label(k)}</option>)}</select></label>
      {draft.act_kind==='DELEGATING'&&<label>Described delegate (if identified)<select disabled={locked} value={draft.delegate_actor_id??''} onChange={e=>change('delegate_actor_id',e.target.value)}><option value="">Not identified</option>{actors.filter(a=>a.actor_id!==draft.subject_actor_id).map(a=><option key={a.actor_id} value={a.actor_id}>{a.display_label}</option>)}</select></label>}
      <label>Characterization<select required value={draft.characterization} onChange={e=>change('characterization',e.target.value)}><option value="">Choose…</option>{characterizations.map(k=><option key={k} value={k}>{label(k)}</option>)}</select></label>
      <label>Context<input required disabled={locked} maxLength={2048} value={draft.context} onChange={e=>change('context',e.target.value)}/></label>
      <label>Temporal scope<input required disabled={locked} maxLength={2048} value={draft.temporal_scope} onChange={e=>change('temporal_scope',e.target.value)}/></label>
      <label>Rationale<textarea required maxLength={4096} value={draft.rationale} onChange={e=>change('rationale',e.target.value)}/></label>
      <label>Evidence fragment<select required={draft.characterization==='OBSERVED'} value={draft.basis_evidence_id??''} onChange={e=>change('basis_evidence_id',e.target.value)}><option value="">Not explicitly linked</option>{evidence.map(e=><option key={e.evidence_id} value={e.evidence_id}>{e.speaker_label}: {e.content?.slice(0,120)} ({e.evidence_id})</option>)}</select></label>
      <button type="submit">Record description only</button>{locked&&<button type="button" onClick={()=>setDraft(empty)}>Cancel revision</button>}{error&&<p role="alert">{error}</p>}
    </fieldset></form>
    <h4>Attributed act history</h4>{events.length?<ul>{events.map(e=><li key={e.event_id}><strong>{actorById.get(e.input.subject_actor_id)??e.input.subject_actor_id} — {label(e.input.act_kind)} ({label(e.input.characterization)})</strong><p>{nodeById.get(e.input.target_object_id)??e.input.target_object_id} · {e.input.context} · {e.input.temporal_scope}</p>{e.input.delegate_actor_id&&<p>Described delegate: {actorById.get(e.input.delegate_actor_id)??e.input.delegate_actor_id}</p>}<p>{e.input.rationale}</p><small>By {e.recorded_by} · {e.recorded_at} · {e.event_id} · basis {e.input.basis_evidence_id??'not explicitly linked'}</small>{revised.has(e.event_id)?<p>Historical — explicitly revised.</p>:<button type="button" disabled={saving} onClick={()=>setDraft({...e.input,revises_event_id:e.event_id})}>Propose revision (original recorder only)</button>}</li>)}</ul>:<p>No explicit act description recorded.</p>}
  </details>;
}
