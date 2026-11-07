import { useState } from 'react';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useClient } from '../hooks/useClient.js';
import { comparisonRequest, initialPlaygroundSettings } from '../state/epistemicPlayground.js';

const numericSettings = [
  ['supportMultiplier', 'Support multiplier (%)', 200],
  ['counterMultiplier', 'Contradiction multiplier (%)', 200],
  ['perOriginCap', 'Maximum contribution per source group', 100],
  ['firstStrength', 'Fictional supporting report A', 100],
  ['copyStrength', 'Fictional supporting report B / copy', 100],
  ['counterStrength', 'Fictional contrary report', 100],
];

function ComparisonResult({ comparison }) {
  const baseline = comparison.baseline;
  const trial = comparison.trial;
  return <article aria-label="Hypothetical weighting comparison">
    <h3>What changed?</h3>
    <p>Baseline source: <strong>{comparison.baseline_policy_source}</strong>. Both columns use exactly the same fictional observations.</p>
    <div className="epistemic-playground-results">
      <div><h4>Current baseline</h4>
        <dl><dt>Support</dt><dd>{baseline.support_points}</dd><dt>Contradiction</dt><dd>{baseline.counter_points}</dd><dt>Net heuristic points</dt><dd>{baseline.net_points}</dd></dl>
      </div>
      <div><h4>Your trial configuration</h4>
        <dl><dt>Support</dt><dd>{trial.support_points}</dd><dt>Contradiction</dt><dd>{trial.counter_points}</dd><dt>Net heuristic points</dt><dd>{trial.net_points}</dd></dl>
      </div>
    </div>
    <p>Difference in net heuristic points: <strong>{comparison.net_points_delta > 0 ? '+' : ''}{comparison.net_points_delta}</strong>.</p>
    <p className="epistemic-boundary">These numbers are not probabilities of truth. Shared documented origins are grouped conservatively. Separate origins do not establish independent witnesses. The 90% human belief, when supplied, is not added to the evidence score.</p>
    <small>Preview only: {comparison.mode}. Not a full reasoning replay, not validated documentary evidence, and no persisted data is changed.</small>
  </article>;
}

export function EpistemicPlaygroundPage() {
  const client = useClient();
  const [settings, setSettings] = useState({ ...initialPlaygroundSettings });
  const [localError, setLocalError] = useState('');
  const [comparisonSession, setComparisonSession] = useState(null);

  const sessionKey = client ? client.config.workspace + ':' + client.config.situation : '';
  const policyQuery = useQuery({
    queryKey: ['epistemic-playground-policy', client?.config.workspace, client?.config.situation],
    enabled: !!client,
    queryFn: async () => (await client.epistemicPolicy()).value,
  });
  const compare = useMutation({
    mutationFn: async (payload) => (await client.compareEpistemic(payload)).value,
  });
  const update = (field, value) => {
    setSettings((current) => ({ ...current, [field]: value }));
    compare.reset();
    setLocalError('');
    setComparisonSession(null);
  };
  const submit = (event) => {
    event.preventDefault();
    if (!client) return;
    try {
      const body = comparisonRequest(settings, client.config.situation);
      setLocalError('');
      compare.mutate(body, {
        onSuccess: () => setComparisonSession(sessionKey),
        onError: () => setComparisonSession(null),
      });
    } catch (error) {
      setLocalError(error.message);
      setComparisonSession(null);
      compare.reset();
    }
  };
  const current = policyQuery.data?.effective_policy;
  const comparison = comparisonSession === sessionKey ? compare.data : null;

  return <section className="epistemic-playground">
    <h2>Reasoning weights — Playground</h2>
    <p>Try different computational heuristics against fictional reports before deciding whether to change a real investigation. No source documents or model calls are involved in this preview.</p>
    {!client ? <p>Connect this session to use the read-only comparison API.</p> : <>
      <article className="epistemic-playground-policy">
        <h3>Current policy for this Situation</h3>
        {policyQuery.isPending && <p>Loading your effective policy…</p>}
        {policyQuery.error && <p className="error">Cannot read the current policy: {policyQuery.error.message}</p>}
        {current && <p>Scope: <strong>{current.scope}</strong> | Support: {current.policy.support_multiplier_pct}% | Contradiction: {current.policy.counter_multiplier_pct}% | Per-origin cap: {current.policy.per_origin_cap}</p>}
      </article>
      <form className="epistemic-playground-form" onSubmit={submit}>
        <fieldset>
          <legend>Your trial weights (never saved)</legend>
          <div className="epistemic-playground-fields">
            {numericSettings.map(([key, label, max]) => <label key={key}>{label}
              <input type="number" min="0" max={max} step="1" required
                value={settings[key]} onChange={(event) => update(key, event.target.value)} />
            </label>)}
            <label>Subjective human confidence (optional, 0–100)
              <input type="number" min="0" max="100" step="1"
                value={settings.humanConfidence} placeholder="Unknown"
                onChange={(event) => update('humanConfidence', event.target.value)} />
            </label>
          </div>
          <label className="epistemic-playground-overlap">
            <input type="checkbox" checked={settings.sharedOrigin}
              onChange={(event) => update('sharedOrigin', event.target.checked)} />
            Reports A and B derive from the same fictional source
          </label>
          <p><small>Uncheck to examine how documented source overlap changes the result. Different sources are not proof of independent witnesses.</small></p>
        </fieldset>
        {localError && <p className="error" role="alert">{localError}</p>}
        {compare.error && <p className="error" role="alert">Comparison unavailable: {compare.error.message}</p>}
        <button type="submit" disabled={compare.isPending}>{compare.isPending ? 'Comparing…' : 'Compare without saving'}</button>
      </form>
      {comparison && <ComparisonResult comparison={comparison} />}
      <details className="technical-details"><summary>What this preview cannot do</summary>
        <p>This is a deterministic score comparison, not a stochastic LLM re-run. The synthetic evidence IDs are not checked against documents. It cannot certify an event, motive, probability or independence, and cannot authorize real-world action.</p>
      </details>
    </>}
  </section>;
}
