export async function launchApprovedReanalysis({ client, priorAnalysis, goal, uuid = () => crypto.randomUUID(), ids = null }) {
  const cleanGoal = goal.trim();
  if (!cleanGoal) throw new Error('Reanalysis goal is required');
  const snapshotOperation = ids?.snapshotOperation ?? uuid();
  const newAnalysis = ids?.newAnalysis ?? uuid();
  const agenticOperation = ids?.agenticOperation ?? uuid();
  const snapshot = await client.runReanalysisSync(snapshotOperation, priorAnalysis, newAnalysis, cleanGoal);
  if (snapshot.value.result?.new_analysis_id !== newAnalysis || snapshot.value.result?.historical_output_preserved !== true) {
    throw new Error('Reanalysis snapshot was not preserved as expected');
  }
  const launch = await client.runIntentAsync(agenticOperation, {
    analysis_id: newAnalysis,
    intent: 'UNDERSTAND',
    goal: cleanGoal,
    reanalysis_of: priorAnalysis,
    idempotency_key: `ui-reanalysis-agentic-${agenticOperation}`,
    step_budget: 32,
    context: { surface: 'review-reanalysis', human_approved: true },
  });
  return { newAnalysis, agenticOperation, launch: launch.value };
}
