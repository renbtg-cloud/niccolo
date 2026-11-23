import { ArtifactReferencePanel } from './ArtifactReferencePanel.jsx';
import { useRuntimeReadiness } from '../hooks/useRuntimeReadiness.js';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useId, useRef, useState } from 'react';
import { Link } from 'react-router-dom';
import { invalidateSituationReview, qk } from '../api/keys.js';
import { useClient } from '../hooks/useClient.js';
import { isCurrentSession, useSessionStore } from '../stores/session.js';
import { situationTextError } from '../state/situationText.js';
import { situationGuide } from '../state/situationGuide.js';

function ContributionItem({ item, pending, onCorrect, onRetract }) {
  const errorId=useId();
  const [text,setText]=useState(item.text);
  const validationError=situationTextError(text);
  return <li className="context-item">
    <div><strong>{item.kind.replaceAll('_',' ')}</strong><div><small>Human context · not verified evidence</small></div></div>
    <textarea aria-label={`Edit context ${item.contribution_id}`} value={text} disabled={pending} aria-invalid={Boolean(validationError)} aria-describedby={validationError?errorId:undefined} onChange={e=>setText(e.target.value)}/>
    {validationError && <p id={errorId} role="alert">{validationError}</p>}
    <div className="row">
      <button disabled={pending||text.trim()===item.text||!text.trim()||Boolean(validationError)} onClick={()=>onCorrect(item,text.trim())}>Save correction</button>
      <button disabled={pending} onClick={()=>onRetract(item)}>Retract</button>
    </div>
  </li>;
}

function FocusForm({ initial = '', label, pending, onSubmit }) {
  const errorId=useId();
  const [value, setValue] = useState(initial);
  const validationError=situationTextError(value);
  return <form aria-busy={pending} onSubmit={(event) => {
    event.preventDefault();
    const focus = value.trim();
    if (focus && !pending && !validationError) onSubmit(focus);
  }}>
    <label>{label}<textarea value={value} disabled={pending} aria-invalid={Boolean(validationError)} aria-describedby={validationError?errorId:undefined} onChange={(event) => setValue(event.target.value)} placeholder="What situation are you trying to understand?"/></label>
    {validationError && <p id={errorId} role="alert">{validationError}</p>}
    <button disabled={pending||!value.trim()||Boolean(validationError)}>{pending ? 'Saving…' : 'Save'}</button>
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
  const generation = useSessionStore(s=>s.generation);
  const pendingContribution = useRef(null);
  const pendingCorrection = useRef(null);
  const [initialLead] = useState(() => useSessionStore.getState().investigationDraft);
  const [contextKind,setContextKind]=useState(initialLead ? 'investigative_preference' : 'user_reported');
  const contextEditor=useRef(null);
  const [contextText,setContextText]=useState(initialLead);
  const contextErrorId=useId();
  const contextError=situationTextError(contextText);

  const situation = useQuery({
    queryKey: client ? qk.situation(client.config) : ['situation','none'],
    enabled: !!client,
    queryFn: async ({ signal }) => (await client.situation(signal)).value,
    retry: false,
  });
  const focusLeadDraft = useRef(Boolean(initialLead));
  const situationLoaded = Boolean(situation.data);
  useEffect(() => {
    if (!focusLeadDraft.current || !situationLoaded) return;
    focusLeadDraft.current = false;
    useSessionStore.getState().stageInvestigationDraft('', generation);
    contextEditor.current?.focus();
  }, [generation, situationLoaded]);
  const readiness = useRuntimeReadiness(client);
  const refresh = async () => {
    if (!isCurrentSession(generation)) return;
    return invalidateSituationReview(qc, client.config);
  };

  const create = useMutation({
    mutationFn: (focus) => client.createSituation(focus),
    onSuccess: refresh,
  });
  const updateFocus = useMutation({
    mutationFn: (focus) => client.setFocus(focus),
    onSuccess: refresh,
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

  if (situation.isLoading) return <p role="status">Loading Situation…</p>;

  if (situation.error?.status === 404) {
    return <section>
      <h2>Start a Situation</h2>
      <p>Give Niccolò the question, concern or project you want to examine. Records can be added immediately afterward.</p>
      <FocusForm label="Initial focus" pending={create.isPending} onSubmit={(focus) => create.mutate(focus)}/>
      {create.error && <p className="error" role="alert">{create.error.message}</p>}
    </section>;
  }
  if (situation.error) return <section>
    <p className="error" role="alert">Could not load the Situation: {situation.error.message}</p>
    <button type="button" disabled={situation.isFetching} onClick={()=>void situation.refetch()}>Retry Situation</button>
  </section>;

  const snapshot = situation.data;
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
      {updateFocus.isSuccess && <p role="status">Situation focus saved.</p>}
      {updateFocus.error && <p className="error" role="alert">{updateFocus.error.message}</p>}
    </details>

    <ArtifactReferencePanel client={client}/>

    <article className="human-context">
      <h3>Human context</h3>
      <p>Add what you remember, a what-if assumption, or an investigative preference. These remain attributed human context and are never silently promoted to evidence.</p>
      <label>Kind<select value={contextKind} disabled={addContext.isPending} onChange={e=>setContextKind(e.target.value)}><option value="user_reported">What I remember / report</option><option value="hypothetical">What if…</option><option value="investigative_preference">What I want Niccolò to examine</option></select></label>
      {initialLead && contextText && contextKind === 'investigative_preference' && !addContext.isSuccess && <p>Review this draft and choose Add context when it reflects what you want to investigate.</p>}
      <label>Context<textarea ref={contextEditor} aria-label="Human context text" value={contextText} disabled={addContext.isPending} aria-invalid={Boolean(contextError)} aria-describedby={contextError?contextErrorId:undefined} onChange={e=>setContextText(e.target.value)} placeholder="Add context in your own words"/></label>
      {contextError&&<p id={contextErrorId} role="alert">{contextError}</p>}
      <button disabled={!contextText.trim()||addContext.isPending||Boolean(contextError)} onClick={()=>addContext.mutate()}>{addContext.isPending?'Adding…':'Add context'}</button>
      {addContext.isSuccess&&<p role="status">Human context recorded.</p>}
      {addContext.error&&<p className="error" role="alert">{addContext.error.message}</p>}
      {snapshot.contributions?.length>0 ? <ul className="context-list">{snapshot.contributions.map((item)=><ContributionItem key={item.contribution_id} item={item} pending={correctContext.isPending||retractContext.isPending} onCorrect={(item,text)=>correctContext.mutate({item,text})} onRetract={(item)=>retractContext.mutate(item)}/>)}</ul> : <p>No human context has been added yet.</p>}
      {correctContext.isSuccess&&<p role="status">Context correction recorded.</p>}
      {correctContext.error&&<p className="error" role="alert">{correctContext.error.message}</p>}
      {retractContext.isSuccess&&<p role="status">Context retracted.</p>}
      {retractContext.error&&<p className="error" role="alert">{retractContext.error.message}</p>}
    </article>

    <article className="situation-guide"><h3>What Niccolò has to work with</h3><ul><li><strong>{guide.focusSet?'Focus set':'Focus missing'}</strong></li><li><strong>{guide.contextCount}</strong> active human-context contribution(s)</li></ul>{!guide.hasGroundingMaterial&&<p>You can run a first pass from the focus alone. Add external artifact references, attributed representations, or human context when they would improve the reasoning.</p>}<small>This is a preparation summary, not a completeness score and not a gate. Artifact references are shown in their dedicated section above.</small></article>
    <article className="next-action"><h3>Ready to reason?</h3>
      {readiness.isPending
        ? <p>Checking reasoning readiness…</p>
        : readiness.error
          ? <div>
              <p className="error" role="alert">Niccolò cannot confirm reasoning readiness: {readiness.error.message}</p>
              <button type="button" disabled={readiness.isFetching} onClick={()=>void readiness.refetch()}>Retry readiness</button>
            </div>
          : readiness.data?.model_configured === false
            ? <><p><strong>A reasoning model is not configured yet.</strong> Your Situation and explicit epistemic material are safe to prepare now, but Analyze needs an OpenAI-compatible model configured on the local service.</p><small>For the local MVP, set NICCOLO_LLM_BASE_URL and NICCOLO_LLM_MODEL before starting, or use .niccolo-local/model.env.</small></>
            : <><p>Start an analysis whenever a first pass would be useful. You can add more artifact references, representations, or context later and reanalyze without rewriting the earlier result.</p><Link to="/operations">Analyze this Situation →</Link></>}
    </article>

    <details className="desktop-only">
      <summary>Raw Situation snapshot</summary>
      <pre>{JSON.stringify(snapshot,null,2)}</pre>
    </details>
  </section>;
}
