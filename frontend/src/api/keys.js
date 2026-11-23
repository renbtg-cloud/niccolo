export const qk = {
  situation: (c) => ['situation', c.workspace, c.situation],
  sources: (c) => ['sources', c.workspace],
  sourceSpeakers: (c, document) => ['source-speakers', c.workspace, document],
  identities: (c) => ['identities', c.workspace],
  identityBindings: (c, document) => ['identity-bindings', c.workspace, document],
  identityHistory: (c, document) => ['identity-history', c.workspace, document],
  revisions: (c) => ['revisions', c.workspace, c.situation],
  analysisHistory: (c) => ['analysis-history', c.workspace, c.situation],
  operation: (c, id) => ['operation', c.workspace, c.situation, id],
  operationEvents: (c, id) => ['operation-events', c.workspace, c.situation, id],
  operationObservation: (c, id) => ['operation-observation', c.workspace, c.situation, id],
  agentic: (c, id) => ['agentic-plan', c.workspace, c.situation, id],
  analysis: (c, id, view) => ['analysis', c.workspace, c.situation, id, view],
  dashboard: (c, id, operation) => ['dashboard', c.workspace, c.situation, id, operation ?? null],
};

export function invalidateSituationReview(queryClient, config) {
  const scope = [config.workspace, config.situation];
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: qk.situation(config) }),
    queryClient.invalidateQueries({ queryKey: ['analysis', ...scope] }),
    queryClient.invalidateQueries({ queryKey: ['dashboard', ...scope] }),
    queryClient.invalidateQueries({ queryKey: qk.analysisHistory(config) }),
  ]);
}
