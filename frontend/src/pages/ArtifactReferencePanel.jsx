import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef, useState } from 'react';
import { qk } from '../api/keys.js';
import { artifactRequest, relationshipRequest, representationRequest, storageReferenceRequest } from '../state/artifacts.js';

const blankArtifact = {artifactType:'',label:'',declaredMimeType:'',declaredByteLength:'',declaredSha256:''};
const blankStorage = {storageType:'UNKNOWN',locator:'',provider:'',environmentLabel:'',resolverProfile:'',notes:'',accessStatus:'UNKNOWN',accessNote:''};
const blankRepresentation = {representationType:'HUMAN_DESCRIPTION',producerType:'USER',producerLabel:'',productionMethod:'',scopeLocator:'whole artifact',content:'',completenessStatus:'PARTIAL',epistemicQualification:'',limitations:''};
const blankRelationship = {subjectArtifactId:'',objectArtifactId:'',relationshipType:'CLAIMED_TO_BE_REPRESENTATION_OF',qualification:''};

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
    <label>Environment label<input value={draft.environmentLabel} onChange={e=>setDraft({...draft,environmentLabel:e.target.value})} placeholder="client production, shared drive…"/></label>
    <label>Resolver profile<input value={draft.resolverProfile} onChange={e=>setDraft({...draft,resolverProfile:e.target.value})} placeholder="human procedure or future adapter name"/></label>
    <label>Location notes<textarea value={draft.notes} onChange={e=>setDraft({...draft,notes:e.target.value})}/></label>
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
    <label>Production method<input value={draft.productionMethod} onChange={e=>setDraft({...draft,productionMethod:e.target.value})} placeholder="typed while watching externally, supplied by Ana…"/></label>
    <label>Scope within artifact<input value={draft.scopeLocator} onChange={e=>setDraft({...draft,scopeLocator:e.target.value})} placeholder="pages 7-9, 00:13:20-00:13:45…"/></label>
    <label>Explicitly supplied text<textarea rows="6" value={draft.content} onChange={e=>setDraft({...draft,content:e.target.value})}/></label>
    <label>Completeness<select value={draft.completenessStatus} onChange={e=>setDraft({...draft,completenessStatus:e.target.value})}><option>PARTIAL</option><option>COMPLETE</option><option>UNKNOWN</option></select></label>
    <label>Qualification<textarea value={draft.epistemicQualification} onChange={e=>setDraft({...draft,epistemicQualification:e.target.value})} placeholder="What this text does and does not establish"/></label>
    <label>Limitations<textarea value={draft.limitations} onChange={e=>setDraft({...draft,limitations:e.target.value})}/></label>
    <button disabled={mutation.isPending||!draft.content.trim()}>{mutation.isPending?'Recording…':'Record attributed representation'}</button>
    {error&&<p className="error" role="alert">{error}</p>}{mutation.error&&<p className="error" role="alert">{mutation.error.message}</p>}
  </form>;
}

function RelationshipForm({client,artifacts,onSaved}){
  const [draft,setDraft]=useState(blankRelationship);
  const [error,setError]=useState(null);
  const pending=useRef(null);
  const mutation=useMutation({
    mutationFn:()=>{
      const body=relationshipRequest(draft);const signature=JSON.stringify(body);
      if(pending.current?.signature!==signature)pending.current={signature,id:crypto.randomUUID()};
      return client.recordArtifactRelationship(pending.current.id,body);
    },
    onSuccess:async()=>{pending.current=null;setDraft(blankRelationship);setError(null);await onSaved();},
  });
  if(artifacts.length<2)return <p>Reference at least two artifacts before declaring a relationship.</p>;
  return <form onSubmit={event=>{event.preventDefault();try{relationshipRequest(draft);setError(null);mutation.mutate();}catch(e){setError(e.message);}}}>
    <label>Subject artifact<select value={draft.subjectArtifactId} onChange={e=>setDraft({...draft,subjectArtifactId:e.target.value})}><option value="">Choose…</option>{artifacts.map(item=><option key={item.artifact_id} value={item.artifact_id}>{item.label}</option>)}</select></label>
    <label>Relationship<select value={draft.relationshipType} onChange={e=>setDraft({...draft,relationshipType:e.target.value})}><option>CLAIMED_TO_BE_REPRESENTATION_OF</option><option>CLAIMED_TO_BE_TRANSCRIPTION_OF</option><option>CLAIMED_TO_BE_DERIVED_FROM</option><option>CLAIMED_TO_BE_COPY_OF</option><option>RELATED_TO</option></select></label>
    <label>Object artifact<select value={draft.objectArtifactId} onChange={e=>setDraft({...draft,objectArtifactId:e.target.value})}><option value="">Choose…</option>{artifacts.map(item=><option key={item.artifact_id} value={item.artifact_id}>{item.label}</option>)}</select></label>
    <label>Qualification<textarea value={draft.qualification} onChange={e=>setDraft({...draft,qualification:e.target.value})} placeholder="Who makes this claim and what remains uncertain?"/></label>
    <button disabled={mutation.isPending}>{mutation.isPending?'Recording…':'Record qualified relationship'}</button>
    {error&&<p className="error" role="alert">{error}</p>}{mutation.error&&<p className="error" role="alert">{mutation.error.message}</p>}
  </form>;
}

export function ArtifactReferencePanel({client,onSummary}){
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
  useEffect(()=>onSummary?.({artifactCount:view.artifacts.length,representationCount:view.representations.length,relationshipCount:view.relationships.length}),[onSummary,view.artifacts.length,view.representations.length,view.relationships.length]);
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
    <details><summary>Relate two artifacts</summary><RelationshipForm client={client} artifacts={view.artifacts} onSaved={refresh}/></details>
    {view.relationships.length>0&&<ul>{view.relationships.map(item=><li key={item.relationship_id}><small>{view.artifacts.find(artifact=>artifact.artifact_id===item.subject_artifact_id)?.label??item.subject_artifact_id} {item.relationship_type} {view.artifacts.find(artifact=>artifact.artifact_id===item.object_artifact_id)?.label??item.object_artifact_id} · unverified declaration{item.qualification?` · ${item.qualification}`:''}</small></li>)}</ul>}
    <small>Recorded access is a human declaration, not proof that Niccolò accessed the location. Representations remain attributable and challengeable.</small>
  </article>;
}
