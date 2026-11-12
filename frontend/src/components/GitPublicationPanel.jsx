import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { qk } from '../api/keys.js';
import { useClient } from '../hooks/useClient.js';
import { useUiStore } from '../stores/ui.js';

export function GitPublicationPanel() {
  const client = useClient();
  const analysis = useUiStore((state) => state.analysis);
  const qc = useQueryClient();
  const pendingTarget = useRef(null);
  const pendingIntent = useRef(null);
  const pendingPublish = useRef(null);
  const [owner, setOwner] = useState('');
  const [repository, setRepository] = useState('');
  const [refName, setRefName] = useState('main');
  const [visibility, setVisibility] = useState('PRIVATE');
  const [selectedTarget, setSelectedTarget] = useState('');
  const [selectedPublication, setSelectedPublication] = useState('');
  const [githubToken, setGithubToken] = useState('');
  const [publishConfirmed, setPublishConfirmed] = useState(false);

  const targets = useQuery({
    queryKey: client ? qk.gitPublicationTargets(client.config) : ['git-publication-targets','none'],
    enabled: !!client,
    queryFn: async () => (await client.gitPublicationTargets()).value,
  });
  const preview = useQuery({
    queryKey: client && analysis && selectedTarget
      ? ['git-publication-preview', client.config.workspace, client.config.situation, analysis, selectedTarget]
      : ['git-publication-preview','none'],
    enabled: !!client && !!analysis && !!selectedTarget,
    queryFn: async () => (await client.gitPublicationPreview(analysis, selectedTarget)).value,
  });
  const intents = useQuery({
    queryKey: client && analysis ? qk.gitPublicationIntents(client.config, analysis) : ['git-publication-intents','none'],
    enabled: !!client && !!analysis,
    queryFn: async () => (await client.gitPublicationIntents(analysis, 25)).value,
  });
  const remoteStatus = useQuery({
    queryKey: client && analysis && selectedPublication
      ? ['git-publication-remote-status', client.config.workspace, client.config.situation, analysis, selectedPublication]
      : ['git-publication-remote-status','none'],
    enabled: !!client && !!analysis && !!selectedPublication,
    queryFn: async () => (await client.gitPublicationRemoteStatus(analysis, selectedPublication)).value,
  });

  const register = useMutation({
    mutationFn: async () => {
      const body = {
        repository_owner: owner.trim(),
        repository_name: repository.trim(),
        ref_name: refName.trim(),
        repository_visibility: visibility,
        sensitive_exports_allowed: false,
      };
      const signature = JSON.stringify(body);
      if (pendingTarget.current?.signature !== signature) {
        pendingTarget.current = { signature, targetId: crypto.randomUUID() };
      }
      return (await client.registerGitPublicationTarget(pendingTarget.current.targetId, body)).value;
    },
    onSuccess: async (value) => {
      const targetId = value.target.target_id;
      pendingTarget.current = null;
      setSelectedTarget(targetId);
      await qc.invalidateQueries({ queryKey: qk.gitPublicationTargets(client.config) });
    },
  });

  const plan = useMutation({
    mutationFn: async () => {
      if (!preview.data || !selectedTarget) throw new Error('Preview this target first.');
      const signature = JSON.stringify({
        analysis,
        target: selectedTarget,
        sha256: preview.data.package_sha256,
        ref: preview.data.requested_ref,
      });
      if (pendingIntent.current?.signature !== signature) {
        pendingIntent.current = { signature, publicationId: crypto.randomUUID() };
      }
      return (await client.recordGitPublicationIntent(
        analysis,
        pendingIntent.current.publicationId,
        selectedTarget,
      )).value;
    },
    onSuccess: async (value) => {
      pendingIntent.current = null;
      setSelectedPublication(value.intent.publication_id);
      setPublishConfirmed(false);
      setGithubToken('');
      await qc.invalidateQueries({ queryKey: qk.gitPublicationIntents(client.config, analysis) });
    },
  });

  const publish = useMutation({
    mutationFn: async () => {
      if (!selectedPublication) throw new Error('Select a recorded publication intent.');
      if (!publishConfirmed) throw new Error('Explicit publication confirmation is required.');
      if (!githubToken) throw new Error('Paste a transient GitHub fine-grained token.');
      const selected = (intents.data?.intents ?? []).find((intent) => intent.publication_id === selectedPublication);
      if (!selected) throw new Error('Selected publication intent is no longer visible.');
      const signature = JSON.stringify({
        publication: selectedPublication,
        package_sha256: selected.package_sha256,
        requested_ref: selected.requested_ref,
      });
      if (pendingPublish.current?.signature !== signature) {
        pendingPublish.current = { signature, attemptId: crypto.randomUUID() };
      }
      return (await client.publishGitPublicationIntent(
        analysis,
        selectedPublication,
        pendingPublish.current.attemptId,
        githubToken,
      )).value;
    },
    onSuccess: async () => {
      pendingPublish.current = null;
      setGithubToken('');
      setPublishConfirmed(false);
      await Promise.all([
        qc.invalidateQueries({ queryKey: qk.gitPublicationIntents(client.config, analysis) }),
        qc.invalidateQueries({ queryKey: ['git-publication-remote-status', client.config.workspace, client.config.situation, analysis, selectedPublication] }),
      ]);
    },
  });

  if (!client || !analysis) return null;
  const rows = targets.data?.targets ?? [];
  const history = intents.data?.intents ?? [];
  const canRegister = owner.trim() && repository.trim() && refName.trim();

  return <article className="review-panel git-publication-panel">
    <h3>GitHub publication planning — no upload yet</h3>
    <p>Record where this portable analysis may later be published. This page stores repository coordinates and an exact package digest only. It does <strong>not</strong> connect to GitHub, accept a provider token, push a commit, or prove remote preservation.</p>
    <p><small>PostgreSQL remains authoritative. Provider authorization and remote commit/ref verification are deliberately required before any future publisher may say “published”.</small></p>

    <details>
      <summary>Add a user-owned GitHub target</summary>
      <label>Owner or organization
        <input value={owner} maxLength={100} onChange={(event) => { pendingTarget.current=null; register.reset(); setOwner(event.target.value); }} placeholder="your-github-owner"/>
      </label>
      <label>Repository
        <input value={repository} maxLength={100} onChange={(event) => { pendingTarget.current=null; register.reset(); setRepository(event.target.value); }} placeholder="niccolo-history"/>
      </label>
      <label>Git ref
        <input value={refName} maxLength={244} onChange={(event) => { pendingTarget.current=null; register.reset(); setRefName(event.target.value); }} />
      </label>
      <label>Declared repository visibility
        <select value={visibility} onChange={(event) => { pendingTarget.current=null; register.reset(); setVisibility(event.target.value); }}>
          <option value="PRIVATE">Private</option>
          <option value="PUBLIC">Public</option>
          <option value="UNKNOWN">Unknown</option>
        </select>
      </label>
      {visibility !== 'PRIVATE' && <p className="historical-note"><strong>Warning:</strong> this target is not declared private. The current planning slice never includes source text and performs no upload.</p>}
      <button disabled={!canRegister || register.isPending} onClick={() => register.mutate()}>{register.isPending?'Recording target…':'Record target metadata'}</button>
      {register.error && <p className="error" role="alert">Target could not be recorded: {register.error.message}</p>}
    </details>

    <label>Publication target
      <select value={selectedTarget} onChange={(event) => { pendingIntent.current=null; plan.reset(); setSelectedTarget(event.target.value); }}>
        <option value="">Select a recorded target</option>
        {rows.map((target) => <option key={target.target_id} value={target.target_id}>
          {target.repository_owner}/{target.repository_name} → {target.ref_name}
        </option>)}
      </select>
    </label>
    {targets.error && <p className="error">Target list unavailable: {targets.error.message}</p>}

    {preview.isPending && <p>Computing exact publication scope…</p>}
    {preview.error && <p className="error">Publication preview unavailable: {preview.error.message}</p>}
    {preview.data && <section className="git-publication-preview" aria-label="Git publication preview">
      <h4>Exact local preview</h4>
      <p><strong>{preview.data.file_count}</strong> file(s) · {preview.data.total_text_bytes} UTF-8 byte(s) · ref <code>{preview.data.requested_ref}</code></p>
      <p>Package SHA-256 <code>{preview.data.package_sha256}</code></p>
      <ul>
        <li>Examined source text included: no</li>
        <li>Original uploads included: no</li>
        <li>Remote publish performed: no</li>
        <li>Provider connection verified: no</li>
      </ul>
      <button disabled={plan.isPending} onClick={() => plan.mutate()}>{plan.isPending?'Recording intent…':'Record publication intent — does not upload'}</button>
      {plan.error && <p className="error" role="alert">Publication intent could not be recorded: {plan.error.message}</p>}
      {plan.isSuccess && <p role="status">Intent recorded locally. GitHub has not been contacted.</p>}
    </section>}

    {selectedPublication && <section className="git-publication-preview" aria-label="Remote GitHub publication">
      <h4>Explicit remote publication</h4>
      <p>This is the network side effect. Niccolò will publish only the exact recorded standard package, use a non-force Git ref update, re-read the ref, and report success only after the remote commit SHA matches.</p>
      <p><small>The GitHub token is transient UI state: it is sent once in a dedicated authorization header and is not written to PostgreSQL, query keys, publication records, URLs, or the returned status.</small></p>
      {remoteStatus.isPending && <p>Checking durable remote status…</p>}
      {remoteStatus.data && <p><strong>{remoteStatus.data.state}</strong>{remoteStatus.data.verified_commit_sha && <> · commit <code>{remoteStatus.data.verified_commit_sha}</code></>}{remoteStatus.data.verified_ref && <> · ref <code>{remoteStatus.data.verified_ref}</code></>}</p>}
      {remoteStatus.error && <p className="error">Remote status unavailable: {remoteStatus.error.message}</p>}
      {remoteStatus.data?.remote_publish_verified ? <p role="status"><strong>Verified published.</strong> PostgreSQL contains a ref-verification observation for the exact remote commit shown above.</p> : <>
        <label>Transient GitHub fine-grained token
          <input type="password" autoComplete="off" value={githubToken} onChange={(event) => { pendingPublish.current=null; publish.reset(); setGithubToken(event.target.value); }} placeholder="Fine-grained token with Contents: write on this repository"/>
        </label>
        <label className="checkbox-label"><input type="checkbox" checked={publishConfirmed} onChange={(event) => { pendingPublish.current=null; publish.reset(); setPublishConfirmed(event.target.checked); }}/> I authorize publishing this exact standard package to the selected GitHub repository/ref.</label>
        <button disabled={!githubToken || !publishConfirmed || publish.isPending} onClick={() => publish.mutate()}>{publish.isPending?'Publishing and verifying remote ref…':'Publish exact package to GitHub and verify'}</button>
        {publish.error && <p className="error" role="alert">GitHub publication was not verified: {publish.error.message}</p>}
        {publish.isSuccess && <p role="status"><strong>Verified published.</strong> Transient token cleared from this UI.</p>}
      </>}
    </section>}

    <details>
      <summary>Recent publication intents ({history.length})</summary>
      {intents.error && <p className="error">Intent history unavailable: {intents.error.message}</p>}
      {history.length===0 ? <p>No publication intent has been recorded for this analysis.</p> :
        <ul>{history.map((intent) => <li key={intent.publication_id}>
          <button className={selectedPublication===intent.publication_id?'history-selected':''} onClick={() => { pendingPublish.current=null; publish.reset(); setGithubToken(''); setPublishConfirmed(false); setSelectedPublication(intent.publication_id); }}><strong>{intent.publication_state}</strong> · select intent</button>
          <div><code>{intent.package_sha256}</code></div>
          <div><small>Ref {intent.requested_ref} · verified remote state is tracked separately · original uploads: no</small></div>
        </li>)}</ul>}
    </details>
  </article>;
}
