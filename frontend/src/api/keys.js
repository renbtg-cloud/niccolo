export const qk = {
  situation: (c) => ['situation', c.workspace, c.situation],
  sources: (c) => ['sources', c.workspace],
  revisions: (c) => ['revisions', c.workspace, c.situation],
  operation: (c, id) => ['operation', c.workspace, c.situation, id],
  operationEvents: (c, id) => ['operation-events', c.workspace, c.situation, id],
  operationObservation: (c, id) => ['operation-observation', c.workspace, c.situation, id],
  agentic: (c, id) => ['agentic-plan', c.workspace, c.situation, id],
  analysis: (c, id, view) => ['analysis', c.workspace, c.situation, id, view],
  dashboard: (c, id, operation) => ['dashboard', c.workspace, c.situation, id, operation ?? null],
};
