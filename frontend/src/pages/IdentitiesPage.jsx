import { useEffect, useMemo, useRef, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { qk } from '../api/keys.js';
import { useClient } from '../hooks/useClient.js';
import { identityMappingSummary } from '../state/identityMappingSummary.js';

function SpeakerRow({ speaker, binding, identities, onAssign, onClear, pending }) {
  const [choice, setChoice] = useState(binding?.person_id ?? '');
  const [newLabel, setNewLabel] = useState('');
  const [reason, setReason] = useState('');

  useEffect(() => {
    setChoice(binding?.person_id ?? '');
    setNewLabel('');
    setReason('');
  }, [binding?.event_id, binding?.person_id]);

  const current = identities.find((item) => item.person_id === binding?.person_id);
  const canAssign = reason.trim() && (choice === '__new__' ? newLabel.trim() : choice);

  return <article className="speaker-row">
    <div className="speaker-heading">
      <div>
        <strong>{speaker.speaker_label}</strong>
        <div><small>{speaker.fragment_count} fragment(s) use this literal source label.</small></div>
      </div>
      <span className={binding?.person_id ? 'status-current' : 'status-historical'}>
        {binding?.person_id ? `Mapped to ${current?.display_label ?? binding.person_id}` : 'Unmapped'}
      </span>
    </div>

    <label>Person
      <select value={choice} onChange={(event) => setChoice(event.target.value)}>
        <option value="">Choose an existing person</option>
        {binding?.person_id && !current && <option value={binding.person_id}>{binding.person_id} · mapped person outside this page</option>}
        {identities.map((identity) => <option key={identity.person_id} value={identity.person_id}>{identity.display_label}</option>)}
        <option value="__new__">Create a new person…</option>
      </select>
    </label>
    {choice === '__new__' && <label>New person label
      <input value={newLabel} onChange={(event) => setNewLabel(event.target.value)} placeholder="Human-confirmed display label"/>
    </label>}
    <label>Why are you making this mapping?
      <textarea value={reason} onChange={(event) => setReason(event.target.value)} placeholder="What lets you identify this speaker label?"/>
    </label>
    <div className="row">
      <button disabled={!canAssign || pending} onClick={() => onAssign({
        speakerLabel: speaker.speaker_label,
        personId: choice === '__new__' ? null : choice,
        newLabel: choice === '__new__' ? newLabel.trim() : null,
        reason: reason.trim(),
      })}>Assign</button>
      {binding?.person_id && <button disabled={!reason.trim() || pending} onClick={() => onClear({
        speakerLabel: speaker.speaker_label,
        reason: reason.trim(),
      })}>Clear mapping</button>}
    </div>
  </article>;
}

export function IdentitiesPage() {
  const client = useClient();
  const qc = useQueryClient();
  const [documentId, setDocumentId] = useState('');
  const [sourceCursor, setSourceCursor] = useState(null);
  const [sourceBackStack, setSourceBackStack] = useState([]);
  const [identityCursor, setIdentityCursor] = useState(null);
  const [identityBackStack, setIdentityBackStack] = useState([]);
  const [historyCursor, setHistoryCursor] = useState(0);
  const [historyBackStack, setHistoryBackStack] = useState([]);
  const pendingAssign = useRef(null);
  const pendingClear = useRef(null);
  const resetDocumentSelection = (id) => {
    pendingAssign.current = null;
    pendingClear.current = null;
    setHistoryCursor(0);
    setHistoryBackStack([]);
    setDocumentId(id);
  };

  const situation = useQuery({
    queryKey: client ? qk.situation(client.config) : ['situation','none'],
    enabled: !!client,
    queryFn: async () => (await client.situation()).value,
  });
  const sources = useQuery({
    queryKey: client ? [...qk.sources(client.config), sourceCursor] : ['sources','none'],
    enabled: !!client,
    queryFn: async () => (await client.sources(sourceCursor, 100)).value,
  });
  const identities = useQuery({
    queryKey: client ? [...qk.identities(client.config), identityCursor] : ['identities','none'],
    enabled: !!client,
    queryFn: async () => (await client.identities(identityCursor, 100)).value,
  });

  const documents = sources.data?.documents ?? [];
  const attachedIds = situation.data?.source_document_ids ?? [];
  const reviewable = documents.filter((document) => attachedIds.includes(document.document_id));

  useEffect(() => {
    if (!documentId && reviewable.length) setDocumentId(reviewable[0].document_id);
  }, [documentId, reviewable]);

  const speakers = useQuery({
    queryKey: client && documentId ? qk.sourceSpeakers(client.config, documentId) : ['source-speakers','none'],
    enabled: !!client && !!documentId,
    queryFn: async () => (await client.sourceSpeakers(documentId)).value,
  });
  const bindings = useQuery({
    queryKey: client && documentId ? qk.identityBindings(client.config, documentId) : ['identity-bindings','none'],
    enabled: !!client && !!documentId,
    queryFn: async () => (await client.effectiveIdentities(documentId)).value,
  });
  const history = useQuery({
    queryKey: client && documentId ? [...qk.identityHistory(client.config, documentId), historyCursor] : ['identity-history','none'],
    enabled: !!client && !!documentId,
    queryFn: async () => (await client.identityHistory(documentId, historyCursor, 100)).value,
  });

  const bindingBySpeaker = useMemo(
    () => new Map((bindings.data?.bindings ?? []).map((binding) => [binding.speaker_label, binding])),
    [bindings.data],
  );
  const speakerRows = speakers.data?.speakers ?? [];
  const mappingSummary = identityMappingSummary(speakerRows, bindings.data?.bindings ?? []);

  const refresh = async () => Promise.all([
    qc.invalidateQueries({ queryKey: qk.identities(client.config) }),
    qc.invalidateQueries({ queryKey: qk.identityBindings(client.config, documentId) }),
    qc.invalidateQueries({ queryKey: qk.identityHistory(client.config, documentId) }),
  ]);

  const assign = useMutation({
    mutationFn: async ({ speakerLabel, personId, newLabel, reason }) => {
      const signature = JSON.stringify({ workspace: client.config.workspace, documentId, speakerLabel, personId, newLabel, reason });
      const newPersonSignature = personId ? null : JSON.stringify({
        workspace: client.config.workspace,
        documentId,
        speakerLabel,
        newLabel,
      });
      if (pendingAssign.current?.signature !== signature) {
        // Creating a person and recording its mapping are separate idempotent
        // HTTP calls. If only the human reason changes after the first call
        // succeeded, reuse that person UUID instead of leaving an orphan.
        const reusablePersonId = newPersonSignature
          && pendingAssign.current?.newPersonSignature === newPersonSignature
          ? pendingAssign.current.personId
          : null;
        pendingAssign.current = {
          signature,
          newPersonSignature,
          eventId: crypto.randomUUID(),
          personId: personId || reusablePersonId || crypto.randomUUID(),
        };
      }
      const attempt = pendingAssign.current;
      if (!personId) {
        await client.createIdentity(attempt.personId, newLabel);
      }
      return (await client.recordIdentityEvent(attempt.eventId, {
        document_id: documentId,
        speaker_label: speakerLabel,
        operation: 'assign',
        person_id: attempt.personId,
        human_reason: reason,
      })).value;
    },
    onSuccess: async () => { pendingAssign.current = null; await refresh(); },
  });
  const clear = useMutation({
    mutationFn: async ({ speakerLabel, reason }) => {
      const signature = JSON.stringify({ workspace: client.config.workspace, documentId, speakerLabel, reason });
      if (pendingClear.current?.signature !== signature) {
        pendingClear.current = { signature, eventId: crypto.randomUUID() };
      }
      return (await client.recordIdentityEvent(pendingClear.current.eventId, {
        document_id: documentId,
        speaker_label: speakerLabel,
        operation: 'clear',
        person_id: null,
        human_reason: reason,
      })).value;
    },
    onSuccess: async () => { pendingClear.current = null; await refresh(); },
  });

  if (!client) return <p>Connect first.</p>;

  const identityRows = identities.data?.identities ?? [];
  return <section>
    <h2>Identity review</h2>
    <p>Speaker labels come from the source artifact. Matching labels are never automatically treated as the same person. Every mapping below is an explicit human action with an append-only reason.</p>

    {reviewable.length === 0 ? <p>Import and attach at least one source to the Situation first.</p> : <label>Document
      <select value={documentId} disabled={assign.isPending || clear.isPending} onChange={(event) => resetDocumentSelection(event.target.value)}>
        {reviewable.map((document) => <option key={document.document_id} value={document.document_id}>{document.locator}</option>)}
      </select>
    </label>}

    <div className="row">
      <button disabled={sources.isFetching || sourceBackStack.length === 0} onClick={() => {
        const stack = sourceBackStack.slice();
        setSourceCursor(stack.pop() ?? null);
        setSourceBackStack(stack);
        resetDocumentSelection('');
      }}>Previous source records</button>
      <button disabled={sources.isFetching || !sources.data?.next_after} onClick={() => {
        setSourceBackStack(stack => [...stack, sourceCursor]);
        setSourceCursor(sources.data.next_after);
        resetDocumentSelection('');
      }}>Next source records</button>
    </div>
    <div className="row">
      <button disabled={identities.isFetching || identityBackStack.length === 0} onClick={() => {
        const stack = identityBackStack.slice();
        setIdentityCursor(stack.pop() ?? null);
        setIdentityBackStack(stack);
      }}>Previous people</button>
      <button disabled={identities.isFetching || !identities.data?.next_after} onClick={() => {
        setIdentityBackStack(stack => [...stack, identityCursor]);
        setIdentityCursor(identities.data.next_after);
      }}>Next people</button>
    </div>

    {(sources.error || situation.error || identities.error || speakers.error || bindings.error || history.error) &&
      <p className="error">{(sources.error || situation.error || identities.error || speakers.error || bindings.error || history.error).message}</p>}

    {documentId&&!speakers.isLoading&&<article className="identity-summary"><strong>{mappingSummary.mappedCount}/{mappingSummary.speakerCount}</strong> source label(s) mapped by a human.{mappingSummary.unmappedCount>0?<p>{mappingSummary.unmappedCount} label(s) remain unmapped. That is allowed; Niccolò will not infer identity merely because labels match.</p>:mappingSummary.speakerCount>0?<p>Every extracted label in this record has an explicit mapping. Coverage does not prove that the mappings are correct.</p>:null}<small>Identity mappings are human assertions with history, not verified facts.</small></article>}

    <div className="speaker-list">
      {speakerRows.map((speaker) => <SpeakerRow
        key={speaker.speaker_label}
        speaker={speaker}
        binding={bindingBySpeaker.get(speaker.speaker_label)}
        identities={identityRows}
        onAssign={(value) => assign.mutate(value)}
        onClear={(value) => clear.mutate(value)}
        pending={assign.isPending || clear.isPending}
      />)}
    </div>
    {documentId && !speakers.isLoading && speakerRows.length === 0 && <p>No speaker labels were extracted from this source.</p>}
    {documentId&&!speakers.isLoading&&speakerRows.length>0&&<p><Link to="/operations">Continue to Analyze →</Link></p>}
    {(assign.error || clear.error) && <p className="error">{(assign.error || clear.error).message}</p>}

    {documentId && <details>
      <summary>Identity mapping history</summary>
      <ol>
        {(history.data?.events ?? []).map((event) => <li key={event.event_id}>
          <strong>{event.speaker_label}</strong> — {event.operation.toLowerCase()}
          {event.person_id ? ` → ${identityRows.find((item) => item.person_id === event.person_id)?.display_label ?? event.person_id}` : ''}
          <div><small>{event.human_reason}</small></div>
        </li>)}
      </ol>
      {(history.data?.events ?? []).length === 0 && <p>No mapping events yet.</p>}
      <div className="row">
        <button disabled={history.isFetching || historyBackStack.length === 0} onClick={() => {
          const stack = historyBackStack.slice();
          setHistoryCursor(stack.pop() ?? 0);
          setHistoryBackStack(stack);
        }}>Previous mapping history</button>
        <button disabled={history.isFetching || !history.data?.next_after} onClick={() => {
          setHistoryBackStack(stack => [...stack, historyCursor]);
          setHistoryCursor(history.data.next_after);
        }}>Next mapping history</button>
      </div>
    </details>}
  </section>;
}
