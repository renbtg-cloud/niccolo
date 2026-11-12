import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { qk } from '../api/keys.js';
import { useClient } from '../hooks/useClient.js';

export function CheckpointMilestonePublicationPanel({ operationId, checkpoint, milestone }) {
  const client = useClient();
  const qc = useQueryClient();
  const pendingIntent = useRef(null);
  const pendingPublish = useRef(null);
  const [selectedTarget, setSelectedTarget] = useState('');
  const [selectedPublication, setSelectedPublication] = useState('');
  const [githubToken, setGithubToken] = useState('');
  const [publishConfirmed, setPublishConfirmed] = useState(false);

  const targets = useQuery({
    queryKey: client ? qk.gitPublicationTargets(client.config) : ['git-publication-targets', 'none'],
    enabled: !!client,
    queryFn: async () => (await client.gitPublicationTargets()).value,
  });
  const preview = useQuery({
    queryKey: client && operationId && checkpoint && milestone && selectedTarget
      ? ['checkpoint-milestone-git-preview', client.config.workspace, client.config.situation, operationId, checkpoint.checkpoint_id, milestone.milestone_id, selectedTarget]
      : ['checkpoint-milestone-git-preview', 'none'],
    enabled: !!client && !!operationId && !!checkpoint && !!milestone && !!selectedTarget,
    queryFn: async () => (await client.operationCheckpointGitPublicationPreview(
      operationId,
      checkpoint.checkpoint_id,
      selectedTarget,
      milestone.milestone_id,
    )).value,
    retry: false,
  });
  const remoteStatus = useQuery({
    queryKey: client && selectedPublication
      ? ['checkpoint-milestone-git-status', client.config.workspace, operationId, checkpoint?.checkpoint_id, selectedPublication]
      : ['checkpoint-milestone-git-status', 'none'],
    enabled: !!client && !!operationId && !!checkpoint && !!selectedPublication,
    queryFn: async () => (await client.operationCheckpointGitPublicationRemoteStatus(
      operationId,
      checkpoint.checkpoint_id,
      selectedPublication,
    )).value,
    retry: false,
  });
  const remoteEvents = useQuery({
    queryKey: client && selectedPublication
      ? ['checkpoint-milestone-git-events', client.config.workspace, operationId, checkpoint?.checkpoint_id, selectedPublication]
      : ['checkpoint-milestone-git-events', 'none'],
    enabled: !!client && !!operationId && !!checkpoint && !!selectedPublication,
    queryFn: async () => (await client.operationCheckpointGitPublicationRemoteEvents(
      operationId,
      checkpoint.checkpoint_id,
      selectedPublication,
      25,
    )).value,
    retry: false,
  });

  const recordIntent = useMutation({
    mutationFn: async () => {
      if (!preview.data || !selectedTarget) throw new Error('Preview a private GitHub target first.');
      const signature = JSON.stringify({
        operationId,
        checkpointId: checkpoint.checkpoint_id,
        milestoneId: milestone.milestone_id,
        targetId: selectedTarget,
        packageSha256: preview.data.package_sha256,
        requestedRef: preview.data.requested_ref,
      });
      if (pendingIntent.current?.signature !== signature) {
        pendingIntent.current = { signature, publicationId: crypto.randomUUID() };
      }
      return (await client.recordOperationCheckpointGitPublicationIntent(
        operationId,
        checkpoint.checkpoint_id,
        pendingIntent.current.publicationId,
        selectedTarget,
        milestone.milestone_id,
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
      if (!selectedPublication) throw new Error('Record the milestone publication intent first.');
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
      return (await client.publishOperationCheckpointGitPublicationIntent(
        operationId,
        checkpoint.checkpoint_id,
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
        qc.invalidateQueries({ queryKey: ['checkpoint-milestone-git-status', client.config.workspace, operationId, checkpoint.checkpoint_id, selectedPublication] }),
        qc.invalidateQueries({ queryKey: ['checkpoint-milestone-git-events', client.config.workspace, operationId, checkpoint.checkpoint_id, selectedPublication] }),
      ]);
    },
  });

  if (!client || !operationId || !checkpoint || !milestone) return null;
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

  return <section className="checkpoint-milestone-publication">
    <h5>Publish this milestone to your GitHub</h5>
    <p><small>This publishes only the exact bounded milestone package. It does not publish original uploads or examined source text, and publication does not make the checkpoint true, complete, or final.</small></p>
    {checkpoint.provisional && <p className="historical-note"><strong>This checkpoint is provisional.</strong> It remains provisional after verified publication.</p>}
    {targets.error && <p className="error">GitHub targets unavailable: {targets.error.message}</p>}
    <label>Private GitHub target
      <select value={selectedTarget} onChange={(event) => changeTarget(event.target.value)}>
        <option value="">Select a recorded private target</option>
        {privateTargets.map((target) => <option key={target.target_id} value={target.target_id}>{target.repository_owner}/{target.repository_name} → {target.ref_name}</option>)}
      </select>
    </label>
    {privateTargets.length === 0 && !targets.isPending && <p><small>No private GitHub target is recorded yet. Add one in Review → User-owned GitHub publication; public/unknown targets are intentionally excluded here.</small></p>}
    {preview.isPending && <p>Calculating exact milestone package digest…</p>}
    {preview.error && <p className="error">Milestone publication preview unavailable: {preview.error.message}</p>}
    {preview.data && <div className="checkpoint-publication-preview">
      <p><strong>Exact package:</strong> <code>{preview.data.package_sha256}</code></p>
      <p><small>{preview.data.scope_kind} · {preview.data.milestone_kind?.replaceAll('_', ' ').toLowerCase()} · ref {preview.data.requested_ref}</small></p>
      <p><small>Reason: {preview.data.milestone_reason}</small></p>
      <p><small>Original uploads: no · examined source text: no · remote upload performed by preview: no.</small></p>
      {!selectedPublication && <button disabled={recordIntent.isPending} onClick={() => recordIntent.mutate()}>{recordIntent.isPending ? 'Recording intent…' : 'Record milestone publication intent — no upload'}</button>}
    </div>}
    {recordIntent.error && <p className="error">Publication intent could not be recorded: {recordIntent.error.message}</p>}
    {selectedPublication && <div className="checkpoint-milestone-publish-confirmation">
      <p><strong>Publication intent recorded.</strong> No remote upload has happened merely by recording it.</p>
      {selected && <p><small>Destination: {selected.repository_owner}/{selected.repository_name} → {selected.ref_name}</small></p>}
      {remoteStatus.data?.remote_publish_verified ? <p role="status"><strong>Verified published.</strong> PostgreSQL records that the remote ref points at the verified commit for this exact milestone package.</p> : <>
        <label>Transient GitHub fine-grained token
          <input type="password" autoComplete="off" value={githubToken} onChange={(event) => { pendingPublish.current = null; publish.reset(); setGithubToken(event.target.value); }} placeholder="Contents: write on this private repository"/>
        </label>
        <label className="checkbox-label"><input type="checkbox" checked={publishConfirmed} onChange={(event) => { pendingPublish.current = null; publish.reset(); setPublishConfirmed(event.target.checked); }}/> I authorize publishing this exact milestone package to the selected private GitHub repository/ref.</label>
        <button disabled={!githubToken || !publishConfirmed || publish.isPending} onClick={() => publish.mutate()}>{publish.isPending ? 'Publishing and verifying…' : 'Publish exact milestone package and verify'}</button>
      </>}
      {publish.error && <p className="error" role="alert">Milestone publication was not verified: {publish.error.message} The transient token was cleared; retry uses the same recorded intent.</p>}
      {remoteStatus.error && <p className="error">Remote milestone status unavailable: {remoteStatus.error.message}</p>}
      {remoteEvents.data?.events?.length > 0 && <details><summary>Milestone remote event log ({remoteStatus.data?.event_count ?? remoteEvents.data.events.length})</summary><ol>{remoteEvents.data.events.map((event) => <li key={`${event.event_id}:${event.event_kind}`}><strong>{event.event_kind}</strong>{event.remote_commit_sha && <> · <code>{event.remote_commit_sha}</code></>}{event.observed_ref && <> · ref <code>{event.observed_ref}</code></>}{event.failure_class && <> · {event.failure_class}</>}</li>)}</ol><small>Only bounded status/commit/ref metadata is stored; provider credentials and response bodies are not persisted.</small></details>}
    </div>}
  </section>;
}
