export class ApiError extends Error { constructor(status, message) { super(message); this.status = status; } }
export function situationPath(c) { return `/v1/workspaces/${c.workspace}/situations/${c.situation}`; }
export function sourcesPath(c) { return `/v1/workspaces/${c.workspace}/sources`; }
export function documentPath(c, id) { return `/v1/workspaces/${c.workspace}/documents/${id}`; }
export function identitiesPath(c) { return `/v1/workspaces/${c.workspace}/identities`; }
export function operationPath(c, id) { return `${situationPath(c)}/bff/operations/${id}`; }
export function intentPath(c, id) { return `${situationPath(c)}/bff/intents/${id}`; }
export function analysisPath(c, id) { return `${situationPath(c)}/analysis-snapshots/${id}`; }
export class NiccoloClient {
  constructor(config, fetchImpl = fetch) { this.config = config; this.fetch = fetchImpl; }
  async request(method, path, { body, etag } = {}) {
    const headers = { Accept: 'application/json', Authorization: `Bearer ${this.config.token}` };
    if (body !== undefined) headers['Content-Type'] = 'application/json';
    if (etag) headers['If-None-Match'] = etag;
    const response = await this.fetch(path, { method, headers, cache: 'no-store', body: body === undefined ? undefined : JSON.stringify(body) });
    if (response.status === 304) return { notModified: true, etag: response.headers.get('etag') ?? etag };
    const value = await response.json().catch(() => ({}));
    if (!response.ok) throw new ApiError(response.status, value.error ?? `HTTP ${response.status}`);
    return { value, etag: response.headers.get('etag'), pollAfterMs: Number(response.headers.get('x-poll-after-ms')) || value.retry_after_ms || null };
  }
  async requestRaw(method, path, { body, contentType, locator } = {}) {
    const headers = { Accept: 'application/json', Authorization: `Bearer ${this.config.token}` };
    if (contentType) headers['Content-Type'] = contentType;
    if (locator) headers['X-Source-Locator'] = locator;
    const response = await this.fetch(path, { method, headers, cache: 'no-store', body });
    const value = await response.json().catch(() => ({}));
    if (!response.ok) throw new ApiError(response.status, value.error ?? `HTTP ${response.status}`);
    return { value, etag: response.headers.get('etag') };
  }
  async downloadSourceOriginal(documentId) {
    const url = `${sourcesPath(this.config)}/${documentId}/original`;
    const response = await this.fetch(url, {
      method: 'GET',
      headers: { Authorization: `Bearer ${this.config.token}`, Accept: 'application/octet-stream' },
      cache: 'no-store',
    });
    if (!response.ok) {
      const error = await response.json().catch(() => ({}));
      throw new ApiError(response.status, error.error ?? `HTTP ${response.status}`);
    }
    return response.blob();
  }
  readiness() { return this.request('GET', '/readyz'); }
  integrationDiagnostics() { return this.request('GET', `/v1/workspaces/${this.config.workspace}/integration-diagnostics`); }
  runtimeResources() { return this.request('GET', `/v1/workspaces/${this.config.workspace}/runtime-resources`); }
  integrationProcessingDeclarations(after = 0, limit = 100) { return this.request('GET', `/v1/workspaces/${this.config.workspace}/integration-processing-declarations?after=${after}&limit=${limit}`); }
  recordIntegrationProcessingDeclaration(declarationId, body) { return this.request('POST', `/v1/workspaces/${this.config.workspace}/integration-processing-declarations/${declarationId}`, { body }); }
  situation() { return this.request('GET', situationPath(this.config)); }
  createSituation(focus) { return this.request('POST', situationPath(this.config), { body: { focus } }); }
  setFocus(focus) { return this.request('POST', `${situationPath(this.config)}/focus`, { body: { focus } }); }
  sources(after = null, limit = 100) { const q = after ? `?after=${after}&limit=${limit}` : `?limit=${limit}`; return this.request('GET', `${sourcesPath(this.config)}${q}`); }
  sourceSpeakers(documentId) { return this.request('GET', `${sourcesPath(this.config)}/${documentId}/speakers`); }
  identities(after = null, limit = 100) { const q = after ? `?after=${after}&limit=${limit}` : `?limit=${limit}`; return this.request('GET', `${identitiesPath(this.config)}${q}`); }
  questions(after = null, limit = 100) { const q = after ? `?after=${after}&limit=${limit}` : `?limit=${limit}`; return this.request('GET', `/v1/workspaces/${this.config.workspace}/questions${q}`); }
  createQuestion(questionId, text) { return this.request('POST', `/v1/workspaces/${this.config.workspace}/questions/${questionId}`, { body: { text } }); }
  questionFeedback(questionId, after = 0, limit = 100) { return this.request('GET', `/v1/workspaces/${this.config.workspace}/questions/${questionId}/feedback?after=${after}&limit=${limit}`); }
  recordQuestionFeedback(questionId, eventId, response, humanReason) { return this.request('POST', `/v1/workspaces/${this.config.workspace}/questions/${questionId}/feedback/${eventId}`, { body: { response, human_reason: humanReason } }); }
  questionPositions(questionId, after = null, limit = 100) { const q = after ? `?after=${after}&limit=${limit}` : `?limit=${limit}`; return this.request('GET', `/v1/workspaces/${this.config.workspace}/questions/${questionId}/positions${q}`); }
  createIdentity(personId, displayLabel) { return this.request('POST', `${identitiesPath(this.config)}/${personId}`, { body: { display_label: displayLabel } }); }
  effectiveIdentities(documentId) { return this.request('GET', `${documentPath(this.config, documentId)}/identities`); }
  identityHistory(documentId, after = 0, limit = 100) { return this.request('GET', `${documentPath(this.config, documentId)}/identity-history?after=${after}&limit=${limit}`); }
  recordIdentityEvent(eventId, body) { return this.request('POST', `/v1/workspaces/${this.config.workspace}/identity-events/${eventId}`, { body }); }
  ingestSource(documentId, { locator, contentType, body }) { return this.requestRaw('POST', `${sourcesPath(this.config)}/${documentId}`, { body, contentType, locator }); }
  intakeReceiptPackage(documentId) { return this.request('GET', `${sourcesPath(this.config)}/${documentId}/intake-receipt-package`); }
  intakeGitPublicationPreview(documentId, targetId) { return this.request('GET', `${sourcesPath(this.config)}/${documentId}/git-publication-preview?target_id=${encodeURIComponent(targetId)}`); }
  recordIntakeGitPublicationIntent(documentId, publicationId, targetId) { return this.request('POST', `${sourcesPath(this.config)}/${documentId}/git-publication-intents/${publicationId}`, { body: { target_id: targetId } }); }
  intakeGitPublicationRemoteStatus(documentId, publicationId) { return this.request('GET', `${sourcesPath(this.config)}/${documentId}/git-publication-intents/${publicationId}/remote-status`); }
  intakeGitPublicationRemoteEvents(documentId, publicationId, limit = 25) { return this.request('GET', `${sourcesPath(this.config)}/${documentId}/git-publication-intents/${publicationId}/remote-events?limit=${limit}`); }
  async publishIntakeGitPublicationIntent(documentId, publicationId, attemptId, githubToken) {
    if (typeof githubToken !== 'string' || !githubToken || githubToken.length > 4096 || /[\r\n\0]/.test(githubToken)) throw new ApiError(400, 'Invalid transient GitHub authorization');
    const path = `${sourcesPath(this.config)}/${documentId}/git-publication-intents/${publicationId}/publish`;
    const response = await this.fetch(path, {
      method: 'POST',
      headers: {
        Accept: 'application/json',
        Authorization: `Bearer ${this.config.token}`,
        'Content-Type': 'application/json',
        'X-Niccolo-GitHub-Authorization': `Bearer ${githubToken}`,
      },
      cache: 'no-store',
      body: JSON.stringify({ attempt_id: attemptId, confirmation: 'PUBLISH_INTAKE_RECEIPT_TO_GITHUB' }),
    });
    const value = await response.json().catch(() => ({}));
    if (!response.ok) throw new ApiError(response.status, value.error ?? `HTTP ${response.status}`);
    return { value };
  }
  addSituationSource(documentId) { return this.request('POST', `${situationPath(this.config)}/sources/${documentId}`); }
  addSituationContribution(contributionId, kind, text, relatedEvidenceId = null) { return this.request('POST', `${situationPath(this.config)}/contributions/${contributionId}`, { body: { kind, text, related_evidence_id: relatedEvidenceId } }); }
  correctSituationContribution(contributionId, replacementId, text, relatedEvidenceId = null) { return this.request('POST', `${situationPath(this.config)}/contributions/${contributionId}/corrections/${replacementId}`, { body: { text, related_evidence_id: relatedEvidenceId } }); }
  retractSituationContribution(contributionId) { return this.request('POST', `${situationPath(this.config)}/contributions/${contributionId}/retract`, { body: {} }); }
  revisions() { return this.request('GET', `${situationPath(this.config)}/revisions?after=0&limit=100`); }
  analysisHistory(before = null, limit = 100) { const q = before ? `?before=${before}&limit=${limit}` : `?limit=${limit}`; return this.request('GET', `${situationPath(this.config)}/analysis-history${q}`); }
  forensicExport() { return this.request('GET', `${situationPath(this.config)}/forensic-export`); }
  forensicImports(before = null, limit = 25) { const q = before ? `?before=${before}&limit=${limit}` : `?limit=${limit}`; return this.request('GET', `/v1/workspaces/${this.config.workspace}/forensic-imports${q}`); }
  forensicImport(batchId) { return this.request('GET', `/v1/workspaces/${this.config.workspace}/forensic-imports/${batchId}`); }
  importForensic(batchId, declaredOrigin, pkg) { return this.request('POST', `/v1/workspaces/${this.config.workspace}/forensic-imports/${batchId}`, { body: { declared_origin: declaredOrigin, package: pkg } }); }
  forensicImportItems(batchId, after = null, limit = 50) { const q = after ? `?after=${encodeURIComponent(after)}&limit=${limit}` : `?limit=${limit}`; return this.request('GET', `/v1/workspaces/${this.config.workspace}/forensic-imports/${batchId}/items${q}`); }
  forensicImportItem(batchId, itemId) { return this.request('GET', `/v1/workspaces/${this.config.workspace}/forensic-imports/${batchId}/items/${itemId}`); }
  async downloadForensicOriginal(batchId, itemId) { const response = await this.fetch(`/v1/workspaces/${this.config.workspace}/forensic-imports/${batchId}/items/${itemId}/original`, { method: 'GET', headers: { Authorization: `Bearer ${this.config.token}`, Accept: 'application/octet-stream' }, cache: 'no-store' }); if (!response.ok) { const error = await response.json().catch(() => ({})); throw new ApiError(response.status, error.error ?? `HTTP ${response.status}`); } return response.blob(); }
  analysisForensicContext(id) { return this.request('GET', `${analysisPath(this.config,id)}/forensic-context`); }
  bindAnalysisForensicContext(id, bindingId, batchId, itemId, note = null) { return this.request('POST', `${analysisPath(this.config,id)}/forensic-context/${bindingId}`, { body: { import_batch_id: batchId, item_id: itemId, note } }); }
  analysisReview(id) { return this.request('GET', `${analysisPath(this.config,id)}/review`); }
  analysisClarifications(id, after = null, limit = 25) { const q = after ? `?after=${after}&limit=${limit}` : `?limit=${limit}`; return this.request('GET', `${analysisPath(this.config,id)}/clarifications${q}`); }
  clarificationAnswers(id, questionId, after = null, limit = 25) { const q = after ? `?after=${after}&limit=${limit}` : `?limit=${limit}`; return this.request('GET', `${analysisPath(this.config,id)}/clarifications/${questionId}/answers${q}`); }
  answerClarification(id, questionId, answerId, text) { return this.request('POST', `${analysisPath(this.config,id)}/clarifications/${questionId}/answers/${answerId}`, { body: { text } }); }
  reanalysisRevision(id) { return this.request('GET', `${analysisPath(this.config,id)}/reanalysis-revision`); }
  analysisCoverage(id) { return this.request('GET', `${analysisPath(this.config,id)}/coverage`); }
  analysisContext(id) { return this.request('GET', `${analysisPath(this.config,id)}/context-usage`); }
  analysisDelta(id) { return this.request('GET', `${analysisPath(this.config,id)}/delta`); }
  analysisOutput(id) { return this.request('GET', `${analysisPath(this.config,id)}/output`); }
  presentationPackage(id) { return this.request('GET', `${analysisPath(this.config,id)}/presentation-package`); }
  presentationSourceExportPreview(id) { return this.request('GET', `${analysisPath(this.config,id)}/presentation-package/source-preview`); }
  presentationExportEvents(id, limit = 25) { return this.request('GET', `${analysisPath(this.config,id)}/presentation-package/export-events?limit=${limit}`); }
  presentationPackageWithExaminedSources(id, exportEventId) { return this.request('POST', `${analysisPath(this.config,id)}/presentation-package`, { body: { include_examined_source_fragments: true, export_event_id: exportEventId } }); }
  gitPublicationTargets() { return this.request('GET', `/v1/workspaces/${this.config.workspace}/git-publication-targets`); }
  registerGitPublicationTarget(targetId, body) { return this.request('POST', `/v1/workspaces/${this.config.workspace}/git-publication-targets/${targetId}`, { body }); }
  async verifyGitPublicationTarget(targetId, githubToken) {
    if (typeof githubToken !== 'string' || !githubToken || githubToken.length > 4096 || /[\r\n\0]/.test(githubToken)) throw new ApiError(400, 'Invalid transient GitHub authorization');
    const response = await this.fetch(`/v1/workspaces/${this.config.workspace}/git-publication-targets/${targetId}/verify`, {
      method: 'POST',
      headers: {
        Accept: 'application/json',
        Authorization: `Bearer ${this.config.token}`,
        'X-Niccolo-GitHub-Authorization': `Bearer ${githubToken}`,
      },
      cache: 'no-store',
    });
    const value = await response.json().catch(() => ({}));
    if (!response.ok) throw new ApiError(response.status, value.error ?? `HTTP ${response.status}`);
    return { value };
  }
  gitPublicationPreview(id, targetId) { return this.request('GET', `${analysisPath(this.config,id)}/git-publication-preview?target_id=${targetId}`); }
  gitPublicationIntents(id, limit = 25) { return this.request('GET', `${analysisPath(this.config,id)}/git-publication-intents?limit=${limit}`); }
  recordGitPublicationIntent(id, publicationId, targetId) { return this.request('POST', `${analysisPath(this.config,id)}/git-publication-intents/${publicationId}`, { body: { target_id: targetId } }); }
  gitPublicationRemoteStatus(id, publicationId) { return this.request('GET', `${analysisPath(this.config,id)}/git-publication-intents/${publicationId}/remote-status`); }
  gitPublicationRemoteEvents(id, publicationId, limit = 25) { return this.request('GET', `${analysisPath(this.config,id)}/git-publication-intents/${publicationId}/remote-events?limit=${limit}`); }
  async publishGitPublicationIntent(id, publicationId, attemptId, githubToken) {
    if (typeof githubToken !== 'string' || !githubToken || githubToken.length > 4096 || /[\r\n\0]/.test(githubToken)) throw new ApiError(400, 'Invalid transient GitHub authorization');
    const path = `${analysisPath(this.config,id)}/git-publication-intents/${publicationId}/publish`;
    const response = await this.fetch(path, {
      method: 'POST',
      headers: {
        Accept: 'application/json',
        Authorization: `Bearer ${this.config.token}`,
        'Content-Type': 'application/json',
        'X-Niccolo-GitHub-Authorization': `Bearer ${githubToken}`,
      },
      cache: 'no-store',
      body: JSON.stringify({ attempt_id: attemptId, confirmation: 'PUBLISH_STANDARD_PRESENTATION_TO_GITHUB' }),
    });
    const value = await response.json().catch(() => ({}));
    if (!response.ok) throw new ApiError(response.status, value.error ?? `HTTP ${response.status}`);
    return { value };
  }
  dashboard(id, operationId) { const q = operationId ? `?operation=${operationId}` : ''; return this.request('GET', `${analysisPath(this.config,id)}/bff/dashboard${q}`); }
  epistemic(id) { return this.request('GET', `${analysisPath(this.config,id)}/bff/epistemic`); }
  epistemicPolicy() { return this.request('GET', `/v1/workspaces/${this.config.workspace}/epistemic/policy?situation_id=${this.config.situation}`); }
  compareEpistemic(body) { return this.request('POST', `/v1/workspaces/${this.config.workspace}/epistemic/compare`, { body }); }
  recordEpistemicPolicy(policyId, body) { return this.request('POST', `/v1/workspaces/${this.config.workspace}/epistemic/policies/${policyId}`, { body }); }
  groundedScoreRecords(id, limit = 100) { return this.request('GET', `${analysisPath(this.config,id)}/epistemic/records?limit=${limit}`); }
  evaluateGroundedScore(id, claimId, body) { return this.request('POST', `${analysisPath(this.config,id)}/claims/${claimId}/epistemic/evaluate`, { body }); }
  recordGroundedScore(id, claimId, scoreId, body) { return this.request('POST', `${analysisPath(this.config,id)}/claims/${claimId}/epistemic/records/${scoreId}`, { body }); }

  reasoningInput(id) { return this.request('GET', `${analysisPath(this.config,id)}/bff/reasoning-input`); }
  reasoningHandoff(id) { return this.request('GET', `${analysisPath(this.config,id)}/bff/handoff`); }
  handoffReceipts(id) { return this.request('GET', `${analysisPath(this.config,id)}/bff/handoff/receipts`); }
  recordHandoffReceipt(id, receiptId, handoffSha256, kind, note = null) { return this.request('POST', `${analysisPath(this.config,id)}/bff/handoff/receipts/${receiptId}`, { body: { handoff_sha256: handoffSha256, kind, note } }); }
  runIntentSync(operationId, task) { return this.request('POST', `${intentPath(this.config,operationId)}/sync`, { body: task }); }
  runIntentAsync(operationId, task) { return this.request('POST', `${intentPath(this.config,operationId)}/async`, { body: task }); }
  runSync(operationId, task) { return this.request('POST', `${operationPath(this.config,operationId)}/sync`, { body: task }); }
  runReanalysisSync(operationId, priorAnalysisId, newAnalysisId, goal) { return this.runSync(operationId, { kind: 'REANALYZE', target_analysis_id: priorAnalysisId, parent_operation_id: null, idempotency_key: `ui-reanalysis-${operationId}`, input: { new_analysis_id: newAnalysisId, goal }, deadline_at: null, step_budget: 32 }); }
  runAsync(operationId, task) { return this.request('POST', `${operationPath(this.config,operationId)}/async`, { body: task }); }
  operation(operationId, etag) { return this.request('GET', operationPath(this.config,operationId), { etag }); }
  waitOperation(operationId, afterVersion, timeoutMs = 5000) { return this.request('GET', `${operationPath(this.config,operationId)}/wait?after_version=${afterVersion}&timeout_ms=${timeoutMs}`); }
  observeOperation(operationId, afterVersion = 0, afterEvent = 0, timeoutMs = 5000, limit = 100) { return this.request('GET', `${operationPath(this.config,operationId)}/observe?after_version=${afterVersion}&after_event=${afterEvent}&timeout_ms=${timeoutMs}&limit=${limit}`); }
  agentic(operationId) { return this.request('GET', `${operationPath(this.config,operationId)}/agentic`); }
  planAgentic(operationId) { return this.request('POST', `${operationPath(this.config,operationId)}/agentic`); }
  respondAgentic(operationId, stepId, body) { return this.request('POST', `${operationPath(this.config,operationId)}/agentic/steps/${stepId}/human-response`, { body }); }
  operationEvents(operationId, after = 0, limit = 50) { return this.request('GET', `${operationPath(this.config,operationId)}/events?after=${after}&limit=${limit}`); }
  operationCheckpoints(operationId, after = 0, limit = 50) { return this.request('GET', `${operationPath(this.config,operationId)}/checkpoints?after=${after}&limit=${limit}`); }
  operationCheckpointPackage(operationId, checkpointId, milestoneId = null) { const milestone = milestoneId ? `?milestone_id=${encodeURIComponent(milestoneId)}` : ''; return this.request('GET', `${operationPath(this.config,operationId)}/checkpoints/${checkpointId}/package${milestone}`); }
  operationCheckpointMilestone(operationId, checkpointId) { return this.request('GET', `${operationPath(this.config,operationId)}/checkpoints/${checkpointId}/milestone`); }
  declareOperationCheckpointMilestone(operationId, checkpointId, milestoneId, milestoneKind, reason) { return this.request('POST', `${operationPath(this.config,operationId)}/checkpoints/${checkpointId}/milestones/${milestoneId}`, { body: { milestone_kind: milestoneKind, reason } }); }
  operationCheckpointGitPublicationPreview(operationId, checkpointId, targetId, milestoneId = null) { const milestone = milestoneId ? `&milestone_id=${encodeURIComponent(milestoneId)}` : ''; return this.request('GET', `${operationPath(this.config,operationId)}/checkpoints/${checkpointId}/git-publication-preview?target_id=${encodeURIComponent(targetId)}${milestone}`); }
  recordOperationCheckpointGitPublicationIntent(operationId, checkpointId, publicationId, targetId, milestoneId = null) { return this.request('POST', `${operationPath(this.config,operationId)}/checkpoints/${checkpointId}/git-publication-intents/${publicationId}`, { body: milestoneId ? { target_id: targetId, milestone_id: milestoneId } : { target_id: targetId } }); }
  operationCheckpointGitPublicationRemoteStatus(operationId, checkpointId, publicationId) { return this.request('GET', `${operationPath(this.config,operationId)}/checkpoints/${checkpointId}/git-publication-intents/${publicationId}/remote-status`); }
  operationCheckpointGitPublicationRemoteEvents(operationId, checkpointId, publicationId, limit = 25) { return this.request('GET', `${operationPath(this.config,operationId)}/checkpoints/${checkpointId}/git-publication-intents/${publicationId}/remote-events?limit=${limit}`); }
  async publishOperationCheckpointGitPublicationIntent(operationId, checkpointId, publicationId, attemptId, githubToken) {
    if (typeof githubToken !== 'string' || !githubToken || githubToken.length > 4096 || /[\r\n\0]/.test(githubToken)) throw new ApiError(400, 'Invalid transient GitHub authorization');
    const path = `${operationPath(this.config,operationId)}/checkpoints/${checkpointId}/git-publication-intents/${publicationId}/publish`;
    const response = await this.fetch(path, {
      method: 'POST',
      headers: {
        Accept: 'application/json',
        Authorization: `Bearer ${this.config.token}`,
        'Content-Type': 'application/json',
        'X-Niccolo-GitHub-Authorization': `Bearer ${githubToken}`,
      },
      cache: 'no-store',
      body: JSON.stringify({ attempt_id: attemptId, confirmation: 'PUBLISH_OPERATION_CHECKPOINT_TO_GITHUB' }),
    });
    const value = await response.json().catch(() => ({}));
    if (!response.ok) throw new ApiError(response.status, value.error ?? `HTTP ${response.status}`);
    return { value };
  }
  operationBudget(operationId) { return this.request('GET', `${operationPath(this.config,operationId)}/budget`); }
  cancelOperation(operationId, expectedVersion, reason) { return this.request('POST', `${operationPath(this.config,operationId)}/cancel`, { body: { expected_version: expectedVersion, reason } }); }
}
