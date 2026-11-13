import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const operations = readFileSync(new URL('./OperationsPage.jsx', import.meta.url), 'utf8');
const situation = readFileSync(new URL('./SituationPage.jsx', import.meta.url), 'utf8');
const app = readFileSync(new URL('../App.jsx', import.meta.url), 'utf8');

describe('primary analysis path', () => {
  it('presents analysis as the primary action while keeping execution plumbing advanced', () => {
    expect(app).toContain('<NavLink to="/">Situation</NavLink><NavLink to="/operations">Analyze</NavLink><NavLink to="/review">Review</NavLink>');
    expect(app).toContain('<summary>Technical</summary>');
    expect(situation).toContain('Analyze this Situation →');
    expect(operations).toContain('<h2>Analyze this Situation</h2>');
    expect(operations).toContain('<summary>Advanced execution</summary>');
    expect(operations.indexOf('<summary>Advanced execution</summary>')).toBeLessThan(operations.indexOf('Reasoning step budget'));
    expect(operations).toContain("run.isPending?'Analyzing…':'Analyze this Situation'");
    expect(operations).toContain('Review reasoning, clarifications and next steps →');
    expect(operations).toContain('<summary>Technical execution details</summary>');
    expect(operations).toContain("operation.state==='FAILED'");
    expect(operations).toContain("operation.state==='CANCELLED'");
    expect(operations).toContain('No answer was published. You can start another analysis.');
  });

  it('blocks analysis when the prepared Situation has not been created, but permits focus-only reasoning', () => {
    expect(operations).toContain('client.situation()');
    expect(operations).toContain('situation.error?.status===404');
    expect(operations).toContain('This prepared Situation has not been created yet.');
    expect(operations).toContain('Start the Situation first →');
    expect(operations).toContain('Focus-only first pass.');
    expect(operations).toContain('This is allowed.');
    expect(operations).toContain('situationMissing||situationUnavailable');
  });

  it('explains unavailable model setup before starting costly analysis work', () => {
    expect(operations).toContain('client.readiness()');
    expect(operations).toContain('readiness.data?.model_configured===false');
    expect(operations).toContain('A reasoning model has not been configured.');
    expect(operations).toContain('run.isPending||modelUnavailable||readiness.isPending||readiness.isError');
  });

  it('exposes bounded model resource budgets only under advanced execution', () => {
    expect(operations).toContain("maxInvocations:32");
    expect(operations).toContain("maxTotalTokens:200000");
    expect(operations).toContain("finalizationReserveMs:5000");
    expect(operations).toContain("max_invocations:v.maxInvocations");
    expect(operations).toContain("max_total_tokens:v.maxTotalTokens");
    expect(operations).toContain("finalization_reserve_ms:v.finalizationReserveMs");
    expect(operations).toContain('hard resource limits, not a quality score');
    expect(operations).toContain('preserves a provisional result');
  });

  it('reuses operation and analysis IDs after a lost response to avoid duplicate billing', () => {
    expect(operations).toContain('const pendingLaunch=useRef(null)');
    expect(operations).toContain('pendingLaunch.current?.signature!==signature');
    expect(operations).toContain('const {operationId,analysisId}=pendingLaunch.current');
    expect(operations).toContain('onSuccess:()=>{pendingLaunch.current=null;}');
    expect(operations).toContain('const pendingHumanResponse=useRef(null)');
    expect(operations).toContain('pendingHumanResponse.current?.signature!==signature');
    expect(operations).toContain('response_id:pendingHumanResponse.current.responseId');
  });

  it('shows live operation-wide budget accounting without presenting it as epistemic quality', () => {
    expect(operations).toContain('client.operationBudget(active)');
    expect(operations).toContain("refetchInterval:observed.operation?.terminal?false:2000");
    expect(operations).toContain('<summary>Reasoning budget</summary>');
    expect(operations).toContain('token_usage_complete');
    expect(operations).toContain('used tokens are a lower bound');
    expect(operations).toContain('not a quality, confidence, or truth score');
  });

  it('shows budget-limited work as provisional instead of pretending synthesis is complete', () => {
    expect(operations).toContain("PARTIAL_BUDGET_BOUNDARY");
    expect(operations).toContain('Useful work so far');
    expect(operations).toContain('does <strong>not</strong> claim completeness or factual truth');
    expect(operations).toContain('Next unexecuted reasoning step');
  });

  it('keeps durable anytime checkpoints in technical inspection with explicit non-truth semantics', () => {
    expect(operations).toContain('useOperationCheckpoints(active)');
    expect(operations).toContain('Anytime checkpoints');
    expect(operations).toContain('A provisional checkpoint is not truth, proof or completeness.');
    expect(operations).toContain('Download latest checkpoint package');
    expect(operations).toContain('client.operationCheckpointPackage(active,checkpoint.checkpoint_id,milestone?.milestone_id??null)');
  });

  it('lets the user inspect the exact milestone package before remote publication', () => {
    expect(operations).toContain('Download exact milestone package');
    expect(operations).toContain('milestone?.milestone_id??null');
  });

  it('offers dedicated explicit publication for the declared milestone', () => {
    expect(operations).toContain('CheckpointMilestonePublicationPanel');
    expect(operations).toContain('operationId={active}');
    expect(operations).toContain('milestone={latestMilestone.data}');
  });

  it('keeps meaningful milestone declaration explicit and non-epistemic', () => {
    expect(operations).toContain('Meaningful milestone');
    expect(operations).toContain('Declare latest checkpoint a milestone');
    expect(operations).toContain('historical label, not an epistemic upgrade');
    expect(operations).toContain('client.operationCheckpointMilestone(active,latestCheckpoint.checkpoint_id)');
    expect(operations).toContain('client.declareOperationCheckpointMilestone(active,latestCheckpoint.checkpoint_id');
  });

  it('surfaces runtime capacity without turning operational pressure into epistemic quality', () => {
    expect(operations).toContain('useRuntimeResources()');
    expect(operations).toContain('RuntimeCapacityPanel');
    expect(operations).toContain("completion_status==='DEGRADED_RESOURCE_PRESSURE'");
    expect(operations).toContain('Reasoning path reduced under critical runtime pressure.');
    expect(operations).toContain('resource pressure is not evidence quality, confidence, completeness or truth');
    expect(operations).toContain('resource_degraded_steps');
  });

  it('keeps observing a transiently complete plan until the durable operation settles', () => {
    expect(operations).toContain('useAgenticPlan(active,observed.operation?.state)');
  });
});
