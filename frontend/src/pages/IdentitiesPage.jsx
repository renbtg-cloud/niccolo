import { useEffect, useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { qk } from '../api/keys.js';
import { useClient } from '../hooks/useClient.js';

function SpeakerRow({ speaker, binding, identities, onAssign, onClear, pending }) {
  const [choice, setChoice] = useState(binding?.person_id ?? '');
  const [newLabel, setNewLabel] = useState('');
  const [reason, setReason] = useState('');

  useEffect(() => {
    setChoice(binding?.person_id ?? '');
  }, [binding?.person_id]);

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

  const situation = useQuery({
    queryKey: client ? qk.situation(client.config) : ['situation','none'],
    enabled: !!client,
    queryFn: async () => (await client.situation()).value,
  });
  const sources = useQuery({
    queryKey: client ? qk.sources(client.config) : ['sources','none'],
    enabled: !!client,
    queryFn: async () => (await client.sources()).value,
  });
  const identities = useQuery({
    queryKey: client ? qk.identities(client.config) : ['identities','none'],
    enabled: !!client,
    queryFn: async () => (await client.identities()).value,
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
    queryKey: client && documentId ? qk.identityHistory(client.config, documentId) : ['identity-history','none'],
    enabled: !!client && !!documentId,
    queryFn: async () => (await client.identityHistory(documentId)).value,
  });

  const bindingBySpeaker = useMemo(
    () => new Map((bindings.data?.bindings ?? []).map((binding) => [binding.speaker_label, binding])),
    [bindings.data],
  );

  const refresh = async () => Promise.all([
    qc.invalidateQueries({ queryKey: qk.identities(client.config) }),
    qc.invalidateQueries({ queryKey: qk.identityBindings(client.config, documentId) }),
    qc.invalidateQueries({ queryKey: qk.identityHistory(client.config, documentId) }),
  ]);

  const assign = useMutation({
    mutationFn: async ({ speakerLabel, personId, newLabel, reason }) => {
      let target = personId;
      if (!target) {
        target = crypto.randomUUID();
        await client.createIdentity(target, newLabel);
      }
      return (await client.recordIdentityEvent(crypto.randomUUID(), {
        document_id: documentId,
        speaker_label: speakerLabel,
        operation: 'assign',
        person_id: target,
        human_reason: reason,
      })).value;
    },
    onSuccess: refresh,
  });
  const clear = useMutation({
    mutationFn: async ({ speakerLabel, reason }) => (await client.recordIdentityEvent(crypto.randomUUID(), {
      document_id: documentId,
      speaker_label: speakerLabel,
      operation: 'clear',
      person_id: null,
      human_reason: reason,
    })).value,
    onSuccess: refresh,
  });

  if (!client) return <p>Connect first.</p>;

  const identityRows = identities.data?.identities ?? [];
  return <section>
    <h2>Identity review</h2>
    <p>Speaker labels come from the source artifact. Matching labels are never automatically treated as the same person. Every mapping below is an explicit human action with an append-only reason.</p>

    {reviewable.length === 0 ? <p>Import and attach at least one source to the Situation first.</p> : <label>Document
      <select value={documentId} onChange={(event) => setDocumentId(event.target.value)}>
        {reviewable.map((document) => <option key={document.document_id} value={document.document_id}>{document.locator}</option>)}
      </select>
    </label>}

    {(sources.error || situation.error || identities.error || speakers.error || bindings.error || history.error) &&
      <p className="error">{(sources.error || situation.error || identities.error || speakers.error || bindings.error || history.error).message}</p>}

    <div className="speaker-list">
      {(speakers.data?.speakers ?? []).map((speaker) => <SpeakerRow
        key={speaker.speaker_label}
        speaker={speaker}
        binding={bindingBySpeaker.get(speaker.speaker_label)}
        identities={identityRows}
        onAssign={(value) => assign.mutate(value)}
        onClear={(value) => clear.mutate(value)}
        pending={assign.isPending || clear.isPending}
      />)}
    </div>
    {documentId && !speakers.isLoading && (speakers.data?.speakers ?? []).length === 0 && <p>No speaker labels were extracted from this source.</p>}
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
    </details>}
  </section>;
}
