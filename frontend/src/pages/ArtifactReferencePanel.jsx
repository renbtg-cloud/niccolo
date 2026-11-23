import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { qk } from '../api/keys.js';
import { artifactRequest, representationRequest, storageReferenceRequest } from '../state/artifacts.js';

const blankArtifact = {artifactType:'',label:'',declaredMimeType:'',declaredByteLength:'',declaredSha256:''};
const blankStorage = {storageType:'UNKNOWN',locator:'',provider:'',environmentLabel:'',resolverProfile:'',notes:'',accessStatus:'UNKNOWN',accessNote:''};
const blankRepresentation = {representationType:'HUMAN_DESCRIPTION',producerType:'USER',producerLabel:'',productionMethod:'',scopeLocator:'whole artifact',content:'',completenessStatus:'PARTIAL',epistemicQualification:'',limitations:''};

function LocationForm({client,artifactId,onSaved}){
  const [draft,setDraft]=useState(blankStorage);
  const [error,setError]=useState(null);
  const pending=useRef(null);
  const mutation=useMutation({
    mutationFn:()=>{
      const signature=JSON.stringify(draft);
      if(pending.current?.signature!==signature) pending.current={signature,referenceId:crypto.randomUUID(),eventId:crypto.randomUUID()};
      return client.recordArtifactStorage(artifactId,pending.current.referenceId,storageReferenceRequest(draft,pending.current.eventId));
    },
    onSuccess:async()=>{pending.current=null;setDraft(blankStorage);setError(null);await onSaved();},
  });
  return <form onSubmit={event=>{event.preventDefault();try{storageReferenceRequest(draft,crypto.randomUUID());setError(null);mutation.mutate();}catch(e){setError(e.message);}}}>
    <label>Storage type<input value={draft.storageType} onChange={e=>setDraft({...draft,storageType:e.target.value})}/></label>
    <label>External locator<input value={draft.locator} onChange={e=>setDraft({...draft,locator:e.target.value})} placeholder="s3://bucket/key or file:///path"/></label>
    <label>Niccolò access<select value={draft.accessStatus} onChange={e=>setDraft({...draft,accessStatus:e.target.value})}><option>UNKNOWN</option><option>NONE</option><option>AVAILABLE</option><option>TEMPORARILY_UNAVAILABLE</option><option>ACCESS_DENIED</option><option>NOT_FOUND</option><option>UNVERIFIED</option></select></label>
    <label>Provider, if useful<input value={draft.provider} onChange={e=>setDraft({...draft,provider:e.target.value})}/></label>
    <label>Access note<textarea value={draft.accessNote} onChange={e=>setDraft({...draft,accessNote:e.target.value})}/></label>
    <button disabled={mutation.isPending||!draft.locator.trim()}>{mutation.isPending?'Recording…':'Record external location'}</button>
    {error&&<p className="error" role="alert">{error}</p>}{mutation.error&&<p className="error" role="alert">{mutation.error.message}</p>}
  </form>;
}

function RepresentationForm({client,artifactId,onSaved}){
  const [draft,setDraft]=useState(blankRepresentation);
  const [error,setError]=useState(null);
  const pending=useRef(null);
  const mutation=useMutation({
    mutationFn:()=>{
      const signature=JSON.stringify(draft);
      if(pending.current?.signature!==signature) pending.current={signature,id:crypto.randomUUID()};
      return client.recordArtifactRepresentation(artifactId,pending.current.id,representationRequest(draft));
    },
    onSuccess:async()=>{pending.current=null;setDraft(blankRepresentation);setError(null);await onSaved();},
  });
  return <form onSubmit={event=>{event.preventDefault();try{representationRequest(draft);setError(null);mutation.mutate();}catch(e){setError(e.message);}}}>
    <label>Representation type<select value={draft.representationType} onChange={e=>setDraft({...draft,representationType:e.target.value})}><option>HUMAN_DESCRIPTION</option><option>HUMAN_TRANSCRIPTION</option><option>SUMMARY</option><option>TABLE_TRANSCRIPTION</option><option>FRAME_DESCRIPTION</option><option>OTHER</option></select></label>
    <label>Who produced it<select value={draft.producerType} onChange={e=>setDraft({...draft,producerType:e.target.value})}><option value="USER">You</option><option value="THIRD_PARTY_HUMAN">Another person</option><option value="EXTERNAL_SYSTEM">External system</option><option value="UNKNOWN">Unknown</option></select></label>
    <label>Producer label<input value={draft.producerLabel} onChange={e=>setDraft({...draft,producerLabel:e.target.value})}/></label>
    <label>Scope within artifact<input value={draft.scopeLocator} onChange={e=>setDraft({...draft,scopeLocator:e.target.value})} placeholder="pages 7-9, 00:13:20-00:13:45…"/></label>
    <label>Explicitly supplied text<textarea rows="6" value={draft.content} onChange={e=>setDraft({...draft,content:e.target.value})}/></label>
    <label>Completeness<select value={draft.completenessStatus} onChange={e=>setDraft({...draft,completenessStatus:e.target.value})}><option>PARTIAL</option><option>COMPLETE</option><option>UNKNOWN</option></select></label>
    <label>Qualification<textarea value={draft.epistemicQualification} onChange={e=>setDraft({...draft,epistemicQualification:e.target.value})} placeholder="What this text does and does not establish"/></label>
    <label>Limitations<textarea value={draft.limitations} onChange={e=>setDraft({...draft,limitations:e.target.value})}/></label>
    <button disabled={mutation.isPending||!draft.content.trim()}>{mutation.isPending?'Recording…':'Record attributed representation'}</button>
    {error&&<p className="error" role="alert">{error}</p>}{mutation.error&&<p className="error" role="alert">{mutation.error.message}</p>}
  </form>;
}

export function ArtifactReferencePanel({client}){
  const qc=useQueryClient();
  const [draft,setDraft]=useState(blankArtifact);
  const [error,setError]=useState(null);
  const pending=useRef(null);
  const query=useQuery({queryKey:qk.artifacts(client.config),queryFn:async({signal})=>(await client.artifacts(signal)).value});
  const refresh=()=>qc.invalidateQueries({queryKey:qk.artifacts(client.config)});
  const create=useMutation({
    mutationFn:()=>{
      const body=artifactRequest(draft);const signature=JSON.stringify(body);
      if(pending.current?.signature!==signature)pending.current={signature,id:crypto.randomUUID()};
      return client.recordArtifact(pending.current.id,body);
    },
    onSuccess:async()=>{pending.current=null;setDraft(blankArtifact);setError(null);await refresh();},
  });
  const view=query.data??{artifacts:[],storage_references:[],representations:[],relationships:[]};
  return <article className="artifact-references">
    <h3>External artifacts</h3>
    <p>Register an artifact without uploading it. Niccolò stores its identity, declared location and what people explicitly report about it. It does not open or parse the artifact.</p>
    <form onSubmit={event=>{event.preventDefault();try{artifactRequest(draft);setError(null);create.mutate();}catch(e){setError(e.message);}}}>
      <label>Artifact type<input value={draft.artifactType} onChange={e=>setDraft({...draft,artifactType:e.target.value})} placeholder="VIDEO, PDF, EMAIL_EXPORT, PHYSICAL_OBJECT…"/></label>
      <label>Label<input value={draft.label} onChange={e=>setDraft({...draft,label:e.target.value})} placeholder="meeting.mp4 or signed contract"/></label>
      <label>Declared MIME type<input value={draft.declaredMimeType} onChange={e=>setDraft({...draft,declaredMimeType:e.target.value})}/></label>
      <label>Declared byte length<input inputMode="numeric" value={draft.declaredByteLength} onChange={e=>setDraft({...draft,declaredByteLength:e.target.value})}/></label>
      <label>Declared SHA-256<input value={draft.declaredSha256} onChange={e=>setDraft({...draft,declaredSha256:e.target.value})}/></label>
      <button disabled={create.isPending||!draft.artifactType.trim()||!draft.label.trim()}>{create.isPending?'Recording…':'Add artifact reference'}</button>
      {error&&<p className="error" role="alert">{error}</p>}{create.error&&<p className="error" role="alert">{create.error.message}</p>}
    </form>
    {query.isLoading&&<p role="status">Loading artifact references…</p>}
    {query.error&&<p className="error" role="alert">{query.error.message}</p>}
    {query.isSuccess&&view.artifacts.length===0&&<p>No external artifacts have been referenced.</p>}
    <ul>{view.artifacts.map(artifact=>{
      const locations=view.storage_references.filter(item=>item.artifact_id===artifact.artifact_id);
      const representations=view.representations.filter(item=>item.artifact_id===artifact.artifact_id);
      return <li key={artifact.artifact_id}><strong>{artifact.label}</strong> <small>{artifact.artifact_type} · original bytes not stored · not inspected by Niccolò</small>
        {locations.map(item=><p key={item.storage_reference_id}><small>{item.storage_type}: {item.locator} · declared access {item.access_status}</small></p>)}
        {representations.map(item=><blockquote key={item.representation_id}><small>{item.representation_type} · {item.scope_locator} · {item.producer_type}</small><p>{item.content}</p></blockquote>)}
        <details><summary>Add external location</summary><LocationForm client={client} artifactId={artifact.artifact_id} onSaved={refresh}/></details>
        <details><summary>Describe or transcribe something in this artifact</summary><RepresentationForm client={client} artifactId={artifact.artifact_id} onSaved={refresh}/></details>
      </li>;
    })}</ul>
    <small>Recorded access is a human declaration, not proof that Niccolò accessed the location. Representations remain attributable and challengeable.</small>
  </article>;
}
