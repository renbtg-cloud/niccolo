import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { qk } from '../api/keys.js';
import { useClient } from '../hooks/useClient.js';
import { useEpistemicView } from '../hooks/useEpistemicView.js';
import { useGroundedScores } from '../hooks/useGroundedScores.js';
import { useUiStore } from '../stores/ui.js';

function sourceDraft(drafts, evidenceId) {
  return drafts[evidenceId] ?? { strength: '50', rationale: '' };
}

function parsedConfidence(raw) {
  const value = String(raw ?? '').trim();
  if (!value) return null;
  const number = Number(value);
  if (!Number.isInteger(number) || number < 0 || number > 100) {
    throw new Error('Human confidence must be an integer from 0 to 100, or left blank.');
  }
  return number;
}

function scoreSummary(record) {
  const report = record?.result?.report ?? {};
  return {
    support: report.support_points ?? 0,
    counter: report.counter_points ?? 0,
    net: report.net_points ?? 0,
  };
}

export function GroundedScorePanel() {
  const pending = useRef(null);
  const client = useClient();
  const analysis = useUiStore((state) => state.analysis);
  const epistemic = useEpistemicView();
  const records = useGroundedScores();
  const qc = useQueryClient();
  const [claimId, setClaimId] = useState('');
  const [drafts, setDrafts] = useState({});
  const [humanConfidence, setHumanConfidence] = useState('');

  const claims = epistemic.data?.claims ?? [];
  const eligible = claims.filter((claim) => (claim.source_provenance ?? []).length > 0);
  const selected = eligible.find((claim) => claim.claim_id === claimId) ?? null;
  const sources = selected?.source_provenance ?? [];
  const complete = !!selected && sources.every((source) => {
    const draft = sourceDraft(drafts, source.evidence_id);
    const strength = Number(draft.strength);
    return Number.isInteger(strength)
      && strength >= 0
      && strength <= 100
      && draft.rationale.trim().length > 0
      && draft.rationale.length <= 4096;
  });

  const record = useMutation({
    mutationFn: async () => {
      if (!client || !analysis || !selected) throw new Error('Select a claim with recorded source links.');
      const confidence = parsedConfidence(humanConfidence);
      const evidence = sources.map((source) => {
        const draft = sourceDraft(drafts, source.evidence_id);
        const strength = Number(draft.strength);
        if (!Number.isInteger(strength) || strength < 0 || strength > 100) {
          throw new Error('Every declared source strength must be an integer from 0 to 100.');
        }
        const rationale = draft.rationale.trim();
        if (!rationale || rationale.length > 4096) {
          throw new Error('Every weighted source needs a human rationale of 1–4096 characters.');
        }
        return {
          evidence_id: source.evidence_id,
          declared_strength: strength,
          rationale,
        };
      });
      const body = {
        human_declared_confidence_percent: confidence,
        evidence,
      };
      const signature = JSON.stringify({ analysis, claim: selected.claim_id, body });
      if (pending.current?.signature !== signature) {
        pending.current = { signature, scoreId: crypto.randomUUID() };
      }
      return (
        await client.recordGroundedScore(
          analysis,
          selected.claim_id,
          pending.current.scoreId,
          body,
        )
      ).value;
    },
    onSuccess: async () => {
      pending.current = null;
      await qc.invalidateQueries({ queryKey: qk.groundedScores(client.config, analysis) });
    },
  });

  if (!client || !analysis) return null;

  const stored = records.data?.records ?? [];
  return <article className="review-panel grounded-score-panel">
    <h3>Record a deterministic heuristic score</h3>
    <p>Optionally freeze one human-directed weighting calculation for audit/export. Niccolò derives each source stance and recorded dependence from the analysis database; you supply only the numeric strength and rationale.</p>
    <p><small>This score is not a probability of truth, proof, documentary evidence, witness-independence certification, or a rewrite of the published answer.</small></p>

    {epistemic.isPending && <p>Loading recorded claims and source links…</p>}
    {epistemic.error && <p className="error">Grounded scoring unavailable: {epistemic.error.message}</p>}
    {!epistemic.isPending && eligible.length === 0 && <p>No claim in this analysis has recorded source links eligible for grounded scoring.</p>}
    {eligible.length > 0 && <>
      <label>Claim
        <select value={claimId} onChange={(event) => {
          pending.current = null;
          record.reset();
          setClaimId(event.target.value);
          setDrafts({});
        }}>
          <option value="">Select a claim</option>
          {eligible.map((claim) => <option key={claim.claim_id} value={claim.claim_id}>
            {claim.ordinal}. {claim.text}
          </option>)}
        </select>
      </label>
      {selected && <div className="grounded-score-sources">
        {sources.map((source) => {
          const draft = sourceDraft(drafts, source.evidence_id);
          return <fieldset key={source.evidence_id}>
            <legend>{source.relation.replaceAll('_', ' ').toLowerCase()}</legend>
            <p><small>{source.locator} · speaker label: {source.speaker_label || 'unknown'} · evidence <code>{source.evidence_id}</code></small></p>
            <label>Human-declared strength (0–100)
              <input type="number" min="0" max="100" step="1" value={draft.strength}
                onChange={(event) => {
                  pending.current = null;
                  record.reset();
                  setDrafts((current) => ({
                    ...current,
                    [source.evidence_id]: { ...draft, strength: event.target.value },
                  }));
                }} />
            </label>
            <label>Why this strength?
              <textarea maxLength={4096} value={draft.rationale}
                placeholder="Your rationale; Niccolò will not invent one."
                onChange={(event) => {
                  pending.current = null;
                  record.reset();
                  setDrafts((current) => ({
                    ...current,
                    [source.evidence_id]: { ...draft, rationale: event.target.value },
                  }));
                }} />
            </label>
          </fieldset>;
        })}
        <label>Subjective human confidence (optional, 0–100)
          <input type="number" min="0" max="100" step="1" value={humanConfidence}
            placeholder="Unknown"
            onChange={(event) => {
              pending.current = null;
              record.reset();
              setHumanConfidence(event.target.value);
            }} />
        </label>
        <button disabled={!complete || record.isPending} onClick={() => record.mutate()}>
          {record.isPending ? 'Recording immutable score…' : 'Record immutable heuristic score'}
        </button>
        {record.error && <p className="error" role="alert">Score could not be recorded: {record.error.message}</p>}
        {record.isSuccess && <p role="status">Score artifact recorded. The published analysis is unchanged.</p>}
      </div>}
    </>}

    <h4>Recorded score artifacts</h4>
    {records.isPending && <p>Loading recorded score artifacts…</p>}
    {records.error && <p className="error">Recorded scores unavailable: {records.error.message}</p>}
    {!records.isPending && stored.length === 0 && <p>No immutable grounded score has been recorded for this analysis.</p>}
    {stored.length > 0 && <ul className="grounded-score-records">{stored.map((item) => {
      const summary = scoreSummary(item);
      return <li key={item.score_id}>
        <strong>Net {summary.net >= 0 ? '+' : ''}{summary.net}</strong>
        <span> support {summary.support} · contradiction {summary.counter}</span>
        <div><small>Claim <code>{item.claim_id}</code> · policy source {item.policy_source} · graph revision {item.result?.source_graph_revision ?? 'unknown'}</small></div>
        <div><small>Result SHA-256 <code>{item.result_sha256}</code> · immutable record: yes · probability of truth: no</small></div>
      </li>;
    })}</ul>}
  </article>;
}
