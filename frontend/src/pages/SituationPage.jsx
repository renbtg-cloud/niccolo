import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { Link } from 'react-router-dom';
import { qk } from '../api/keys.js';
import { useClient } from '../hooks/useClient.js';
import { sourceDescriptor } from '../state/sourceUpload.js';

function ContributionItem({ item, pending, onCorrect, onRetract }) {
  const [text,setText]=useState(item.text);
  return <li className="context-item">
    <div><strong>{item.kind.replaceAll('_',' ')}</strong><div><small>Human context · not verified evidence</small></div></div>
    <textarea aria-label={`Edit context ${item.contribution_id}`} value={text} onChange={e=>setText(e.target.value)}/>
    <div className="row">
      <button disabled={pending||text.trim()===item.text||!text.trim()} onClick={()=>onCorrect(item,text.trim())}>Save correction</button>
      <button disabled={pending} onClick={()=>onRetract(item)}>Retract</button>
    </div>
  </li>;
}

function FocusForm({ initial = '', label, pending, onSubmit }) {
  const [value, setValue] = useState(initial);
  return <form onSubmit={(event) => {
    event.preventDefault();
    const focus = value.trim();
    if (focus) onSubmit(focus);
  }}>
    <label>{label}<textarea value={value} onChange={(event) => setValue(event.target.value)} placeholder="What situation are you trying to understand?"/></label>
    <button disabled={pending}>{pending ? 'Saving…' : 'Save'}</button>
  </form>;
}

export function SituationPage(){
  const client = useClient();
  const qc = useQueryClient();
  const [file, setFile] = useState(null);
  const [contextKind,setContextKind]=useState('user_reported');
  const [contextText,setContextText]=useState('');

  const situation = useQuery({
    queryKey: client ? qk.situation(client.config) : ['situation','none'],
    enabled: !!client,
    queryFn: async () => (await client.situation()).value,
    retry: false,
  });
  const sources = useQuery({
    queryKey: client ? qk.sources(client.config) : ['sources','none'],
    enabled: !!client,
    queryFn: async () => (await client.sources()).value,
  });
  const refresh = async () => Promise.all([
    qc.invalidateQueries({ queryKey: qk.situation(client.config) }),
    qc.invalidateQueries({ queryKey: qk.sources(client.config) }),
  ]);

  const create = useMutation({
    mutationFn: (focus) => client.createSituation(focus),
    onSuccess: refresh,
  });
  const updateFocus = useMutation({
    mutationFn: (focus) => client.setFocus(focus),
    onSuccess: refresh,
  });
  const attach = useMutation({
    mutationFn: (documentId) => client.addSituationSource(documentId),
    onSuccess: refresh,
  });
  const addContext = useMutation({
    mutationFn: async () => client.addSituationContribution(crypto.randomUUID(), contextKind, contextText.trim(), null),
    onSuccess: async () => { setContextText(''); await refresh(); },
  });
  const correctContext = useMutation({
    mutationFn: ({item,text}) => client.correctSituationContribution(item.contribution_id, crypto.randomUUID(), text, item.related_evidence_id ?? null),
    onSuccess: refresh,
  });
  const retractContext = useMutation({
    mutationFn: (item) => client.retractSituationContribution(item.contribution_id),
    onSuccess: refresh,
  });
  const upload = useMutation({
    mutationFn: async (selected) => {
      const descriptor = sourceDescriptor(selected);
      const documentId = crypto.randomUUID();
      const ingested = await client.ingestSource(documentId, {
        locator: descriptor.locator,
        contentType: descriptor.contentType,
        body: selected,
      });
      const attached = await client.addSituationSource(documentId);
      return { documentId, ingest: ingested.value, situation: attached.value };
    },
    onSuccess: async () => {
      setFile(null);
      await refresh();
    },
  });

  if (!client) return <p>Connect to the BFF first.</p>;
  if (situation.isLoading) return <p>Loading Situation…</p>;

  if (situation.error?.status === 404) {
    return <section>
      <h2>Start a Situation</h2>
      <p>Give Niccolò the question, concern or project you want to examine. Records can be added immediately afterward.</p>
      <FocusForm label="Initial focus" pending={create.isPending} onSubmit={(focus) => create.mutate(focus)}/>
      {create.error && <p className="error">{create.error.message}</p>}
    </section>;
  }
  if (situation.error) return <p className="error">{situation.error.message}</p>;

  const snapshot = situation.data;
  const attached = new Set(snapshot?.source_document_ids ?? []);
  const documents = sources.data?.documents ?? [];

  return <section>
    <h2>Situation</h2>
    <article className="situation-summary">
      <div><strong>Current focus</strong><p>{snapshot.focus}</p></div>
      <small>Revision {snapshot.revision}. Earlier analysis inputs remain frozen rather than being silently rewritten.</small>
    </article>

    <details>
      <summary>Change focus</summary>
      <FocusForm initial={snapshot.focus} label="Focus" pending={updateFocus.isPending} onSubmit={(focus) => updateFocus.mutate(focus)}/>
      {updateFocus.error && <p className="error">{updateFocus.error.message}</p>}
    </details>

    <article className="source-upload">
      <h3>Add records</h3>
      <p>Upload an email (.eml) or a plain-text WhatsApp/text export (.txt). Niccolò preserves the uploaded bytes and attribution boundaries; a speaker label is not automatically a verified identity.</p>
      <input
        aria-label="Source file"
        type="file"
        accept=".eml,.txt,message/rfc822,text/plain"
        onChange={(event) => setFile(event.target.files?.[0] ?? null)}
      />
      <button disabled={!file || upload.isPending} onClick={() => file && upload.mutate(file)}>
        {upload.isPending ? 'Importing…' : 'Import and attach'}
      </button>
      {file && <small>Selected: {file.name} · {file.size} bytes</small>}
      {upload.error && <p className="error">{upload.error.message}</p>}
      {upload.data && <p>Imported document <code>{upload.data.documentId}</code>.</p>}
    </article>

    <article>
      <h3>Source catalog</h3>
      {sources.isLoading && <p>Loading sources…</p>}
      {sources.error && <p className="error">{sources.error.message}</p>}
      {!sources.isLoading && documents.length === 0 && <p>No source records have been imported yet.</p>}
      <ul className="source-list">
        {documents.map((document) => <li key={document.document_id}>
          <div>
            <strong>{document.locator}</strong>
            <div><small>{document.format} · {document.fragment_count} attributed fragment(s) · {document.byte_length} bytes</small></div>
          </div>
          {attached.has(document.document_id)
            ? <span className="status-current">Attached</span>
            : <button disabled={attach.isPending} onClick={() => attach.mutate(document.document_id)}>Attach to Situation</button>}
        </li>)}
      </ul>
      {attach.error && <p className="error">{attach.error.message}</p>}
    </article>

    <article className="human-context">
      <h3>Human context</h3>
      <p>Add what you remember, a what-if assumption, or an investigative preference. These remain attributed human context and are never silently promoted to evidence.</p>
      <label>Kind<select value={contextKind} onChange={e=>setContextKind(e.target.value)}><option value="user_reported">What I remember / report</option><option value="hypothetical">What if…</option><option value="investigative_preference">What I want Niccolò to examine</option></select></label>
      <label>Context<textarea aria-label="Human context text" value={contextText} onChange={e=>setContextText(e.target.value)} placeholder="Add context in your own words"/></label>
      <button disabled={!contextText.trim()||addContext.isPending} onClick={()=>addContext.mutate()}>{addContext.isPending?'Adding…':'Add context'}</button>
      {addContext.error&&<p className="error">{addContext.error.message}</p>}
      {snapshot.contributions?.length>0 ? <ul className="context-list">{snapshot.contributions.map((item)=><ContributionItem key={item.contribution_id} item={item} pending={correctContext.isPending||retractContext.isPending} onCorrect={(item,text)=>correctContext.mutate({item,text})} onRetract={(item)=>retractContext.mutate(item)}/>)}</ul> : <p>No human context has been added yet.</p>}
      {correctContext.error&&<p className="error">{correctContext.error.message}</p>}
      {retractContext.error&&<p className="error">{retractContext.error.message}</p>}
    </article>

    <article className="next-action"><h3>Ready to reason?</h3><p>When the Situation has enough context for a useful pass, start an analysis. You can add more records or context later and reanalyze without rewriting the earlier result.</p><Link to="/operations">Analyze this Situation →</Link></article>

    <details className="desktop-only">
      <summary>Raw Situation snapshot</summary>
      <pre>{JSON.stringify(snapshot,null,2)}</pre>
    </details>
  </section>;
}
