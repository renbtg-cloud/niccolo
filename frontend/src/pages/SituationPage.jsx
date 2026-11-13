import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { Link } from 'react-router-dom';
import { SourceIntakePublicationPanel } from '../components/SourceIntakePublicationPanel.jsx';
import { qk } from '../api/keys.js';
import { useClient } from '../hooks/useClient.js';
import { useSessionStore } from '../stores/session.js';
import { sourceBatchDescriptors } from '../state/sourceUpload.js';
import { situationGuide } from '../state/situationGuide.js';

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
  const generation = useSessionStore((state) => state.generation);
  if (!client) return <p>Connect this session first.</p>;
  return <ConnectedSituation
    key={generation}
    client={client}
  />;
}

function ConnectedSituation({client}){
  const qc = useQueryClient();
  const [files, setFiles] = useState([]);
  const [uploadProgress, setUploadProgress] = useState(null);
  const [sourceCursor, setSourceCursor] = useState(null);
  const [sourceBackStack, setSourceBackStack] = useState([]);
  // Preserve each document identity across retries after a partial multi-file import.
  const pendingDocumentIds = useRef(new Map());
  const sourceInputRef = useRef(null);
  const pendingContribution = useRef(null);
  const pendingCorrection = useRef(null);
  const [contextKind,setContextKind]=useState('user_reported');
  const [contextText,setContextText]=useState('');

  const situation = useQuery({
    queryKey: client ? qk.situation(client.config) : ['situation','none'],
    enabled: !!client,
    queryFn: async () => (await client.situation()).value,
    retry: false,
  });
  const sources = useQuery({
    queryKey: client ? [...qk.sources(client.config), sourceCursor] : ['sources','none'],
    enabled: !!client,
    queryFn: async () => (await client.sources(sourceCursor, 100)).value,
  });
  const readiness = useQuery({
    queryKey: ['mvp-runtime-readiness'],
    enabled: !!client,
    queryFn: async () => (await client.readiness()).value,
    staleTime: 60000,
    retry: false,
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
  const downloadOriginal = useMutation({
    mutationFn: async (source) => {
      const blob = await client.downloadSourceOriginal(source.document_id);
      const ext = source.format === 'EML' ? 'eml' : 'txt';
      const url = URL.createObjectURL(blob);
      try {
        const link = document.createElement('a');
        link.href = url;
        link.download = `niccolo-source-${source.document_id}.${ext}`;
        document.body.append(link);
        link.click();
        link.remove();
      } finally {
        window.setTimeout(() => URL.revokeObjectURL(url), 30000);
      }
    },
  });
  const addContext = useMutation({
    mutationFn: async () => {
      const text = contextText.trim();
      const signature = JSON.stringify({ kind: contextKind, text });
      if (pendingContribution.current?.signature !== signature) {
        pendingContribution.current = { signature, id: crypto.randomUUID() };
      }
      return client.addSituationContribution(pendingContribution.current.id, contextKind, text, null);
    },
    onSuccess: async () => { pendingContribution.current = null; setContextText(''); await refresh(); },
  });
  const correctContext = useMutation({
    mutationFn: ({item,text}) => {
      const signature = JSON.stringify({ contribution: item.contribution_id, text, evidence: item.related_evidence_id ?? null });
      if (pendingCorrection.current?.signature !== signature) {
        pendingCorrection.current = { signature, id: crypto.randomUUID() };
      }
      return client.correctSituationContribution(item.contribution_id, pendingCorrection.current.id, text, item.related_evidence_id ?? null);
    },
    onSuccess: async () => { pendingCorrection.current = null; await refresh(); },
  });
  const retractContext = useMutation({
    mutationFn: (item) => client.retractSituationContribution(item.contribution_id),
    onSuccess: refresh,
  });
  const upload = useMutation({
    mutationFn: async (selectedFiles) => {
      const descriptors = sourceBatchDescriptors(selectedFiles);
      const imported = [];
      setUploadProgress({ done: 0, total: selectedFiles.length, current: null });
      for (const [index, selected] of selectedFiles.entries()) {
        const descriptor = descriptors[index];
        setUploadProgress({ done: index, total: selectedFiles.length, current: selected.name });
        // Keep one id per actual File object. Two selected files can have the
        // same name/size/mtime yet different bytes; metadata keys would alias
        // them and turn the second upload into a false idempotency conflict.
        let documentId = pendingDocumentIds.current.get(selected);
        if (!documentId) {
          documentId = crypto.randomUUID();
          pendingDocumentIds.current.set(selected, documentId);
        }
        const ingested = await client.ingestSource(documentId, {
          locator: descriptor.locator,
          contentType: descriptor.contentType,
          body: selected,
        });
        const attached = await client.addSituationSource(documentId);
        imported.push({ documentId, ingest: ingested.value, situation: attached.value });
        setUploadProgress({ done: index + 1, total: selectedFiles.length, current: null });
      }
      return { imported };
    },
    onSuccess: async () => {
      setFiles([]);
      setUploadProgress(null);
      pendingDocumentIds.current.clear();
      if (sourceInputRef.current) sourceInputRef.current.value = '';
      await refresh();
    },
    // If a later file fails after earlier files succeeded, preserve every
    // document UUID so retrying the same selection replays rather than duplicates.
    onError: refresh,
  });

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
  const guide = situationGuide(snapshot);

  return <section>
    <h2>Situation</h2>
    <article className="situation-summary">
      <div><strong>Current focus</strong><p>{snapshot.focus}</p></div>
      <small>Revision {snapshot.revision}. Earlier analysis inputs remain frozen rather than being silently rewritten.</small>
    </article>

    <details>
      <summary>Change focus</summary>
      <FocusForm
        key={JSON.stringify([client.config.workspace, client.config.situation, snapshot.focus])}
        initial={snapshot.focus}
        label="Focus"
        pending={updateFocus.isPending}
        onSubmit={(focus) => updateFocus.mutate(focus)}
      />
      {updateFocus.error && <p className="error">{updateFocus.error.message}</p>}
    </details>

    <article className="source-upload">
      <h3>Add records</h3>
      <p>Upload a plain-text email (.eml) or a WhatsApp chat export (.txt). Generic text files are not interpreted as WhatsApp conversations. Niccolò preserves the uploaded bytes and attribution boundaries; a speaker label is not automatically a verified identity.</p>
      <input
        ref={sourceInputRef}
        aria-label="Source file"
        type="file"
        multiple
        accept=".eml,.txt,message/rfc822,text/plain"
        disabled={upload.isPending}
        onChange={(event) => {
          pendingDocumentIds.current.clear();
          upload.reset();
          setUploadProgress(null);
          setFiles(Array.from(event.target.files ?? []));
        }}
      />
      <button disabled={files.length === 0 || upload.isPending} onClick={() => files.length && upload.mutate(files)}>
        {upload.isPending ? `Importing ${files.length} record(s)…` : `Import and attach ${files.length || ''} record${files.length === 1 ? '' : 's'}`}
      </button>
      {files.length > 0 && <div><small>Selected {files.length} file(s) · {files.reduce((sum, selected) => sum + selected.size, 0)} bytes total</small><ul>{files.slice(0,8).map((selected) => <li key={`${selected.name}:${selected.size}:${selected.lastModified}`}><small>{selected.name} · {selected.size} bytes</small></li>)}</ul>{files.length > 8 && <small>+ {files.length - 8} more file(s)</small>}</div>}
      {uploadProgress && <p role="status"><small>Imported and attached {uploadProgress.done}/{uploadProgress.total}{uploadProgress.current ? <> · processing <strong>{uploadProgress.current}</strong></> : ''}.</small></p>}
      {upload.error && <p className="error">{upload.error.message} Successfully completed files remain attached and will replay safely if you retry the same selection.</p>}
      {upload.data && <p>{upload.data.imported.length} record(s) imported and attached to this Situation.</p>}
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
            {document.original_bytes_available === false
              ? <div><small className="error">Original bytes unavailable (legacy import). Re-upload the same file to restore byte-exact retention.</small></div>
              : document.original_bytes_available === true
                ? <div><small>Original bytes preserved</small></div>
                : null}
          </div>
          {attached.has(document.document_id)
            ? <span className="status-current">Attached</span>
            : <button disabled={attach.isPending} onClick={() => attach.mutate(document.document_id)}>Attach to Situation</button>}
          {document.original_bytes_available&&<button disabled={downloadOriginal.isPending} onClick={()=>downloadOriginal.mutate(document)}>Download original</button>}
          <SourceIntakePublicationPanel document={document}/>
        </li>)}
      </ul>
      <div className="row">
        <button disabled={sources.isFetching || sourceBackStack.length === 0} onClick={() => {
          const stack = sourceBackStack.slice();
          setSourceCursor(stack.pop() ?? null);
          setSourceBackStack(stack);
        }}>Previous sources</button>
        <button disabled={sources.isFetching || !sources.data?.next_after} onClick={() => {
          setSourceBackStack(stack => [...stack, sourceCursor]);
          setSourceCursor(sources.data.next_after);
        }}>Next sources</button>
      </div>
      {attach.error && <p className="error">{attach.error.message}</p>}
      {downloadOriginal.error && <p className="error" role="alert">{downloadOriginal.error.message}</p>}
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

    <article className="situation-guide"><h3>What Niccolò has to work with</h3><ul><li><strong>{guide.focusSet?'Focus set':'Focus missing'}</strong></li><li><strong>{guide.sourceCount}</strong> attached record(s)</li><li><strong>{guide.contextCount}</strong> active human-context contribution(s)</li></ul>{!guide.hasGroundingMaterial&&<p>You can run a first pass from the focus alone. Add records or human context when they would make the reasoning better grounded.</p>}<small>This is a preparation summary, not a completeness score and not a gate.</small>{guide.sourceCount>0&&<p><Link to="/identities">Review who source labels refer to →</Link></p>}</article>
    <article className="next-action"><h3>Ready to reason?</h3>
      {readiness.isPending
        ? <p>Checking reasoning readiness…</p>
        : readiness.error
          ? <p className="error">Niccolò cannot confirm reasoning readiness: {readiness.error.message}</p>
          : readiness.data?.model_configured === false
            ? <><p><strong>A reasoning model is not configured yet.</strong> Your Situation and records are safe to prepare now, but Analyze needs an OpenAI-compatible model configured on the local service.</p><small>For the local MVP, run bash scripts/configure-local-model.sh once, then restart Niccolò. The private settings are kept in .niccolo-local/model.env.</small></>
            : <><p>Start an analysis whenever a first pass would be useful. You can add more records or context later and reanalyze without rewriting the earlier result.</p><Link to="/operations">Analyze this Situation →</Link></>}
    </article>

    <details className="desktop-only">
      <summary>Raw Situation snapshot</summary>
      <pre>{JSON.stringify(snapshot,null,2)}</pre>
    </details>
  </section>;
}
