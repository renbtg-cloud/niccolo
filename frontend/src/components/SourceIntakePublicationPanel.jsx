import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { qk } from '../api/keys.js';
import { useClient } from '../hooks/useClient.js';

export function SourceIntakePublicationPanel({ document }) {
  const client = useClient();
  const qc = useQueryClient();
  const pendingIntent = useRef(null);
  const pendingPublish = useRef(null);
  const [open, setOpen] = useState(false);
  const [selectedTarget, setSelectedTarget] = useState('');
  const [selectedPublication, setSelectedPublication] = useState('');
  const [githubToken, setGithubToken] = useState('');
  const [publishConfirmed, setPublishConfirmed] = useState(false);

  const targets = useQuery({
    queryKey: client ? qk.gitPublicationTargets(client.config) : ['git-publication-targets', 'none'],
    enabled: !!client && open,
    queryFn: async () => (await client.gitPublicationTargets()).value,
  });
  const receipt = useQuery({
    queryKey: client && document?.document_id
      ? ['intake-receipt-package', client.config.workspace, document.document_id]
      : ['intake-receipt-package', 'none'],
    enabled: !!client && !!document?.document_id && open,
    queryFn: async () => (await client.intakeReceiptPackage(document.document_id)).value,
    retry: false,
  });
  const preview = useQuery({
    queryKey: client && document?.document_id && selectedTarget
      ? ['intake-git-preview', client.config.workspace, document.document_id, selectedTarget]
      : ['intake-git-preview', 'none'],
    enabled: !!client && !!document?.document_id && !!selectedTarget && open,
    queryFn: async () => (await client.intakeGitPublicationPreview(document.document_id, selectedTarget)).value,
    retry: false,
  });
  const remoteStatus = useQuery({
    queryKey: client && document?.document_id && selectedPublication
      ? ['intake-git-status', client.config.workspace, document.document_id, selectedPublication]
      : ['intake-git-status', 'none'],
    enabled: !!client && !!document?.document_id && !!selectedPublication && open,
    queryFn: async () => (await client.intakeGitPublicationRemoteStatus(document.document_id, selectedPublication)).value,
    retry: false,
  });
  const remoteEvents = useQuery({
    queryKey: client && document?.document_id && selectedPublication
      ? ['intake-git-events', client.config.workspace, document.document_id, selectedPublication]
      : ['intake-git-events', 'none'],
    enabled: !!client && !!document?.document_id && !!selectedPublication && open,
    queryFn: async () => (await client.intakeGitPublicationRemoteEvents(document.document_id, selectedPublication, 25)).value,
    retry: false,
  });

  const recordIntent = useMutation({
    mutationFn: async () => {
      if (!preview.data || !selectedTarget) throw new Error('Preview a private GitHub target first.');
      const signature = JSON.stringify({
        documentId: document.document_id,
        targetId: selectedTarget,
        packageSha256: preview.data.package_sha256,
        requestedRef: preview.data.requested_ref,
      });
      if (pendingIntent.current?.signature !== signature) {
        pendingIntent.current = { signature, publicationId: crypto.randomUUID() };
      }
      return (await client.recordIntakeGitPublicationIntent(
        document.document_id,
        pendingIntent.current.publicationId,
        selectedTarget,
      )).value;
    },
    onSuccess: (value) => {
      pendingIntent.current = null;
      pendingPublish.current = null;
      setSelectedPublication(value.intent.publication_id);
      setGithubToken('');
      setPublishConfirmed(false);
    },
  });

  const publish = useMutation({
    mutationFn: async () => {
      if (!selectedPublication) throw new Error('Record the receipt publication intent first.');
      if (!publishConfirmed) throw new Error('Explicit publication confirmation is required.');
      if (!githubToken) throw new Error('Paste a transient GitHub fine-grained token.');
      const signature = JSON.stringify({
        publicationId: selectedPublication,
        packageSha256: preview.data?.package_sha256 ?? null,
        requestedRef: preview.data?.requested_ref ?? null,
      });
      if (pendingPublish.current?.signature !== signature) {
        pendingPublish.current = { signature, attemptId: crypto.randomUUID() };
      }
      return (await client.publishIntakeGitPublicationIntent(
        document.document_id,
        selectedPublication,
        pendingPublish.current.attemptId,
        githubToken,
      )).value;
    },
    onError: () => {
      setGithubToken('');
      setPublishConfirmed(false);
    },
    onSuccess: async () => {
      pendingPublish.current = null;
      setGithubToken('');
      setPublishConfirmed(false);
      await Promise.all([
        qc.invalidateQueries({ queryKey: ['intake-git-status', client.config.workspace, document.document_id, selectedPublication] }),
        qc.invalidateQueries({ queryKey: ['intake-git-events', client.config.workspace, document.document_id, selectedPublication] }),
      ]);
    },
  });

  if (!client || !document?.document_id) return null;
  const privateTargets = (targets.data?.targets ?? []).filter((target) => target.repository_visibility === 'PRIVATE');
  const selected = privateTargets.find((target) => target.target_id === selectedTarget) ?? null;
  const changeTarget = (targetId) => {
    pendingIntent.current = null;
    pendingPublish.current = null;
    recordIntent.reset();
    publish.reset();
    setSelectedPublication('');
    setGithubToken('');
    setPublishConfirmed(false);
    setSelectedTarget(targetId);
  };

  return <details className="intake-publication" onToggle={(event) => setOpen(event.currentTarget.open)}>
    <summary>Publish receipt to private Git history</summary>
    <p><small>This receipt records only that Niccolò received bytes with a digest and parsed them. It does not publish the original upload or source text, and it does not establish authenticity, truth, completeness, independence, or analysis.</small></p>
    {receipt.isPending && <p>Calculating deterministic receipt…</p>}
    {receipt.error && <p className="error">Receipt unavailable: {receipt.error.message}</p>}
    {receipt.data && <div>
      <p><strong>Received source digest:</strong> <code>{receipt.data.receipt?.source_sha256}</code></p>
      <p><strong>Receipt package:</strong> <code>{receipt.data.package_sha256}</code></p>
      <p><small>State {receipt.data.receipt?.state} · original upload in receipt: no · source text in receipt: no · analysis in receipt: no.</small></p>
    </div>}

    <label>Private GitHub target
      <select value={selectedTarget} onChange={(event) => changeTarget(event.target.value)}>
        <option value="">Select a recorded private target</option>
        {privateTargets.map((target) => <option key={target.target_id} value={target.target_id}>{target.repository_owner}/{target.repository_name} → {target.ref_name}</option>)}
      </select>
    </label>
    {privateTargets.length === 0 && !targets.isPending && <p><small>No private GitHub target is recorded. Add one in Review → User-owned GitHub publication. Public or unknown targets are intentionally excluded.</small></p>}

    {preview.isPending && <p>Previewing exact receipt publication…</p>}
    {preview.error && <p className="error">Receipt publication preview unavailable: {preview.error.message}</p>}
    {preview.data && <div>
      <p><strong>Exact package to publish:</strong> <code>{preview.data.package_sha256}</code></p>
      <p><small>{preview.data.scope_kind} · ref {preview.data.requested_ref} · remote upload performed by preview: no.</small></p>
      <p><small>Authenticity established: no · truth established: no · completeness proved: no.</small></p>
      {!selectedPublication && <button disabled={recordIntent.isPending} onClick={() => recordIntent.mutate()}>{recordIntent.isPending ? 'Recording intent…' : 'Record receipt publication intent — no upload'}</button>}
    </div>}
    {recordIntent.error && <p className="error">Receipt publication intent could not be recorded: {recordIntent.error.message}</p>}

    {selectedPublication && <div>
      <p><strong>Publication intent recorded.</strong> The source remains merely RECEIVED; no remote upload happened by recording the intent.</p>
      {selected && <p><small>Destination: {selected.repository_owner}/{selected.repository_name} → {selected.ref_name}</small></p>}
      {remoteStatus.data?.remote_publish_verified ? <p role="status"><strong>Verified published.</strong> PostgreSQL records that the remote ref points at the verified commit for this exact receipt package. The source still remains RECEIVED.</p> : <>
        <label>Transient GitHub fine-grained token
          <input type="password" autoComplete="off" value={githubToken} onChange={(event) => { pendingPublish.current = null; publish.reset(); setGithubToken(event.target.value); }} placeholder="Contents: write on this private repository"/>
        </label>
        <label className="checkbox-label"><input type="checkbox" checked={publishConfirmed} onChange={(event) => { pendingPublish.current = null; publish.reset(); setPublishConfirmed(event.target.checked); }}/> I authorize publishing this exact RECEIVED-source receipt to the selected private GitHub repository/ref.</label>
        <button disabled={!githubToken || !publishConfirmed || publish.isPending} onClick={() => publish.mutate()}>{publish.isPending ? 'Publishing and verifying…' : 'Publish receipt and verify remote ref'}</button>
      </>}
      {publish.error && <p className="error" role="alert">Receipt publication was not verified: {publish.error.message} The transient token was cleared; retry keeps the recorded intent.</p>}
      {remoteStatus.error && <p className="error">Remote receipt status unavailable: {remoteStatus.error.message}</p>}
      {remoteEvents.data?.events?.length > 0 && <details><summary>Receipt remote event log ({remoteStatus.data?.event_count ?? remoteEvents.data.events.length})</summary><ol>{remoteEvents.data.events.map((event) => <li key={`${event.event_id}:${event.event_kind}`}><strong>{event.event_kind}</strong>{event.remote_commit_sha && <> · <code>{event.remote_commit_sha}</code></>}{event.observed_ref && <> · ref <code>{event.observed_ref}</code></>}{event.failure_class && <> · {event.failure_class}</>}</li>)}</ol><small>Only bounded status/commit/ref metadata is stored; provider credentials and response bodies are not persisted.</small></details>}
    </div>}
  </details>;
}
