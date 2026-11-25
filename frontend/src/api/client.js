export class ApiError extends Error { constructor(status, message) { super(message); this.status = status; } }
export function situationPath(c) { return `/v1/workspaces/${c.workspace}/situations/${c.situation}`; }
export function sourcesPath(c) { return `/v1/workspaces/${c.workspace}/sources`; }
export function documentPath(c, id) { return `/v1/workspaces/${c.workspace}/documents/${id}`; }
export function identitiesPath(c) { return `/v1/workspaces/${c.workspace}/identities`; }
export function operationPath(c, id) { return `${situationPath(c)}/bff/operations/${id}`; }
export function intentPath(c, id) { return `${situationPath(c)}/bff/intents/${id}`; }
export function analysisPath(c, id) { return `${situationPath(c)}/analysis-snapshots/${id}`; }
export function artifactsPath(c) { return `${situationPath(c)}/artifacts`; }
export class NiccoloClient {
  constructor(config, fetchImpl = fetch) { this.config = config; this.fetch = fetchImpl; }
  async request(method, path, { body, etag, signal } = {}) {
    const headers = { Accept: 'application/json', Authorization: `Bearer ${this.config.token}` };
    if (body !== undefined) headers['Content-Type'] = 'application/json';
    if (etag) headers['If-None-Match'] = etag;
    const response = await this.fetch(path, { method, headers, cache: 'no-store', signal, body: body === undefined ? undefined : JSON.stringify(body) });
    if (response.status === 304) return { notModified: true, etag: response.headers.get('etag') ?? etag };
    const value = await response.json().catch((error) => {
      if (signal?.aborted || error?.name === 'AbortError') throw error;
      return {};
    });
    if (!response.ok) throw new ApiError(response.status, value.error ?? `HTTP ${response.status}`);
    return { value, etag: response.headers.get('etag'), pollAfterMs: Number(response.headers.get('x-poll-after-ms')) || value.retry_after_ms || null };
  }
  readiness(signal) { return this.request('GET', '/readyz', { signal }); }
  situation(signal) { return this.request('GET', situationPath(this.config), { signal }); }
  createSituation(focus) { return this.request('POST', situationPath(this.config), { body: { focus } }); }
  setFocus(focus) { return this.request('POST', `${situationPath(this.config)}/focus`, { body: { focus } }); }
  artifacts(signal) { return this.request('GET', artifactsPath(this.config), { signal }); }
  recordArtifact(artifactId, body) { return this.request('POST', `${artifactsPath(this.config)}/${artifactId}`, { body }); }
  recordArtifactStorage(artifactId, referenceId, body) { return this.request('POST', `${artifactsPath(this.config)}/${artifactId}/storage-references/${referenceId}`, { body }); }
  recordArtifactRepresentation(artifactId, representationId, body) { return this.request('POST', `${artifactsPath(this.config)}/${artifactId}/representations/${representationId}`, { body }); }
  recordArtifactRelationship(relationshipId, body) { return this.request('POST', `${situationPath(this.config)}/artifact-relationships/${relationshipId}`, { body }); }
  sources(after = null, limit = 100, signal) { const q = after ? `?after=${after}&limit=${limit}` : `?limit=${limit}`; return this.request('GET', `${sourcesPath(this.config)}${q}`, { signal }); }
  sourceSpeakers(documentId, signal) { return this.request('GET', `${sourcesPath(this.config)}/${documentId}/speakers`, { signal }); }
  identities(after = null, limit = 100, signal) { const q = after ? `?after=${after}&limit=${limit}` : `?limit=${limit}`; return this.request('GET', `${identitiesPath(this.config)}${q}`, { signal }); }
  questions(after = null, limit = 100, signal) { const q = after ? `?after=${after}&limit=${limit}` : `?limit=${limit}`; return this.request('GET', `/v1/workspaces/${this.config.workspace}/questions${q}`, { signal }); }
  createQuestion(questionId, text) { return this.request('POST', `/v1/workspaces/${this.config.workspace}/questions/${questionId}`, { body: { text } }); }
  questionFeedback(questionId, after = 0, limit = 100, signal) { return this.request('GET', `/v1/workspaces/${this.config.workspace}/questions/${questionId}/feedback?after=${after}&limit=${limit}`, { signal }); }
  recordQuestionFeedback(questionId, eventId, response, humanReason) { return this.request('POST', `/v1/workspaces/${this.config.workspace}/questions/${questionId}/feedback/${eventId}`, { body: { response, human_reason: humanReason } }); }
  questionPositions(questionId, after = null, limit = 100, signal) { const q = after ? `?after=${after}&limit=${limit}` : `?limit=${limit}`; return this.request('GET', `/v1/workspaces/${this.config.workspace}/questions/${questionId}/positions${q}`, { signal }); }
  createIdentity(personId, displayLabel) { return this.request('POST', `${identitiesPath(this.config)}/${personId}`, { body: { display_label: displayLabel } }); }
  effectiveIdentities(documentId, signal) { return this.request('GET', `${documentPath(this.config, documentId)}/identities`, { signal }); }
  identityHistory(documentId, after = 0, limit = 100, signal) { return this.request('GET', `${documentPath(this.config, documentId)}/identity-history?after=${after}&limit=${limit}`, { signal }); }
  recordIdentityEvent(eventId, body) { return this.request('POST', `/v1/workspaces/${this.config.workspace}/identity-events/${eventId}`, { body }); }
  addSituationContribution(contributionId, kind, text, relatedEvidenceId = null) { return this.request('POST', `${situationPath(this.config)}/contributions/${contributionId}`, { body: { kind, text, related_evidence_id: relatedEvidenceId } }); }
  correctSituationContribution(contributionId, replacementId, text, relatedEvidenceId = null) { return this.request('POST', `${situationPath(this.config)}/contributions/${contributionId}/corrections/${replacementId}`, { body: { text, related_evidence_id: relatedEvidenceId } }); }
  retractSituationContribution(contributionId) { return this.request('POST', `${situationPath(this.config)}/contributions/${contributionId}/retract`, { body: {} }); }
  revisions() { return this.request('GET', `${situationPath(this.config)}/revisions?after=0&limit=100`); }
  analysisHistory(before = null, limit = 100, signal) { const q = before ? `?before=${before}&limit=${limit}` : `?limit=${limit}`; return this.request('GET', `${situationPath(this.config)}/analysis-history${q}`, { signal }); }
  analysisReview(id, signal) { return this.request('GET', `${analysisPath(this.config,id)}/review`, { signal }); }
  analysisClarifications(id, after = null, limit = 25, signal) { const q = after ? `?after=${after}&limit=${limit}` : `?limit=${limit}`; return this.request('GET', `${analysisPath(this.config,id)}/clarifications${q}`, { signal }); }
  clarificationAnswers(id, questionId, after = null, limit = 25, signal) { const q = after ? `?after=${after}&limit=${limit}` : `?limit=${limit}`; return this.request('GET', `${analysisPath(this.config,id)}/clarifications/${questionId}/answers${q}`, { signal }); }
  answerClarification(id, questionId, answerId, text) { return this.request('POST', `${analysisPath(this.config,id)}/clarifications/${questionId}/answers/${answerId}`, { body: { text } }); }
  sourceRequests(id, signal) { return this.request('GET', `${analysisPath(this.config,id)}/source-requests`, { signal }); }
  recordSourceRequestEvent(id, requestId, eventId, kind, note) { return this.request('POST', `${analysisPath(this.config,id)}/source-requests/${requestId}/events/${eventId}`, { body: { kind, note } }); }
  reanalysisRevision(id, signal) { return this.request('GET', `${analysisPath(this.config,id)}/reanalysis-revision`, { signal }); }
  analysisCoverage(id, signal) { return this.request('GET', `${analysisPath(this.config,id)}/coverage`, { signal }); }
  analysisContext(id, signal, after = null) {
    const query = after ? `?after=${encodeURIComponent(after)}` : '';
    return this.request('GET', `${analysisPath(this.config,id)}/context-usage${query}`, { signal });
  }
  analysisDelta(id, signal) { return this.request('GET', `${analysisPath(this.config,id)}/delta`, { signal }); }
  analysisOutput(id, signal) { return this.request('GET', `${analysisPath(this.config,id)}/output`, { signal }); }
  presentationPackage(id) { return this.request('GET', `${analysisPath(this.config,id)}/presentation-package`); }
  dashboard(id, operationId, signal) { const q = operationId ? `?operation=${operationId}` : ''; return this.request('GET', `${analysisPath(this.config,id)}/bff/dashboard${q}`, { signal }); }
  epistemic(id, signal) { return this.request('GET', `${analysisPath(this.config,id)}/bff/epistemic`, { signal }); }
  epistemicPolicy(signal) { return this.request('GET', `/v1/workspaces/${this.config.workspace}/epistemic/policy?situation_id=${this.config.situation}`, { signal }); }
  compareEpistemic(body) { return this.request('POST', `/v1/workspaces/${this.config.workspace}/epistemic/compare`, { body }); }
  recordEpistemicPolicy(policyId, body) { return this.request('POST', `/v1/workspaces/${this.config.workspace}/epistemic/policies/${policyId}`, { body }); }
  expertWorkbench(signal) { return this.request('GET', `${situationPath(this.config)}/epistemic/workbench`, { signal }); }
  draftExpertInput(text) { return this.request('POST', `${situationPath(this.config)}/epistemic/workbench/input-draft`, { body: { text } }); }
  adoptExpertInputDraft(draftId, reviewed) { return this.request('POST', `${situationPath(this.config)}/epistemic/workbench/input-draft/${draftId}`, { body: reviewed }); }
  expertInputDraft(draftId) { return this.request('GET', `${situationPath(this.config)}/epistemic/workbench/input-draft/${draftId}`); }
  recordInquiryDecision(decisionId, body) { return this.request('POST', `${situationPath(this.config)}/epistemic/question-decisions/${decisionId}`, {body}); }
  recordDecisionBasisReview(decisionId, eventId, body) { return this.request('POST', `${situationPath(this.config)}/epistemic/question-decisions/${decisionId}/reviews/${eventId}`, {body}); }
  recordActionRecommendation(recommendationId, body) { return this.request('POST', `${situationPath(this.config)}/epistemic/action-recommendations/${recommendationId}`, {body}); }
  recordActionDecision(decisionId, body) { return this.request('POST', `${situationPath(this.config)}/epistemic/action-decisions/${decisionId}`, {body}); }
  recordIntervention(recordId, body) { return this.request('POST', `${situationPath(this.config)}/epistemic/intervention-records/${recordId}`, {body}); }
  recordInterventionAssessment(assessmentId, body) { return this.request('POST', `${situationPath(this.config)}/epistemic/intervention-causal-assessments/${assessmentId}`, {body}); }
  recordInterventionReview(reviewId, body) { return this.request('POST', `${situationPath(this.config)}/epistemic/intervention-causal-reviews/${reviewId}`, {body}); }
  recordActorEpistemicState(eventId, body) { return this.request('POST', `${situationPath(this.config)}/epistemic/actor-states/${eventId}`, {body}); }
  recordNormativeAssessment(eventId, body) { return this.request('POST', `${situationPath(this.config)}/epistemic/normative-assessments/${eventId}`, {body}); }
  recordActorEpistemicAct(eventId, body) { return this.request('POST', `${situationPath(this.config)}/epistemic/actor-acts/${eventId}`, {body}); }
  actors(signal) { return this.request('GET', `/v1/workspaces/${this.config.workspace}/actors`, { signal }); }
  recordExpertQuestionResponse(questionId, responseId, body) { return this.request('POST', `${situationPath(this.config)}/epistemic/questions/${questionId}/responses/${responseId}`, { body }); }
  recordResponseInterpretation(utteranceId, interpretationId, revisionId, body) { return this.request('POST', `${situationPath(this.config)}/epistemic/utterances/${utteranceId}/interpretations/${interpretationId}/revisions/${revisionId}`, { body }); }
  confirmResponseInterpretation(utteranceId, interpretationId, revisionId, eventId, body) { return this.request('POST', `${situationPath(this.config)}/epistemic/utterances/${utteranceId}/interpretations/${interpretationId}/revisions/${revisionId}/confirmations/${eventId}`, { body }); }
  recordInterpretationDependency(dependencyId, body) { return this.request('POST', `${situationPath(this.config)}/epistemic/interpretation-dependencies/${dependencyId}`, { body }); }
  reviewInterpretationDependency(dependencyId,eventId,body) { return this.request('POST', `${situationPath(this.config)}/epistemic/interpretation-dependencies/${dependencyId}/reviews/${eventId}`, {body}); }
  searchExpertEvidence(query, limit = 20) { return this.request('GET', `${situationPath(this.config)}/epistemic/workbench/evidence/search?q=${encodeURIComponent(query)}&limit=${limit}`); }
  createExpertNode(objectId, body) { return this.request('POST', `${situationPath(this.config)}/epistemic/objects/${objectId}`, { body }); }
  createExpertRelation(relationId, body) { return this.request('POST', `${situationPath(this.config)}/epistemic/relations/${relationId}`, { body }); }
  createExpertArgument(argumentId, body) { return this.request('POST', `${situationPath(this.config)}/epistemic/arguments/${argumentId}`, { body }); }
  createExpertGrounding(argumentId, groundingId, body) { return this.request('POST', `${situationPath(this.config)}/epistemic/arguments/${argumentId}/grounding/${groundingId}`, { body }); }
  recordExpertApplicability(argumentId, groundingId, eventId, body) { return this.request('POST', `${situationPath(this.config)}/epistemic/arguments/${argumentId}/grounding/${groundingId}/applicability/${eventId}`, { body }); }
  recordExpertAssessment(eventId, body) { return this.request('POST', `${situationPath(this.config)}/epistemic/assessments/${eventId}`, { body }); }
  recordExpertAssessmentDependency(dependencyId, body) { return this.request('POST', `${situationPath(this.config)}/epistemic/assessment-dependencies/${dependencyId}`, { body }); }
  reviewExpertAssessmentDependency(dependencyId, eventId, body) { return this.request('POST', `${situationPath(this.config)}/epistemic/assessment-dependencies/${dependencyId}/reviews/${eventId}`, { body }); }
  reviewExpertAssessmentDownstream(dependencyId, eventId, body) { return this.request('POST', `${situationPath(this.config)}/epistemic/assessment-dependencies/${dependencyId}/downstream-reviews/${eventId}`, { body }); }
  reviewExpertReassessment(triggerRelationId, argumentId, eventId, body) { return this.request('POST', `${situationPath(this.config)}/epistemic/reassessment/${triggerRelationId}/arguments/${argumentId}/reviews/${eventId}`, { body }); }

  reasoningInput(id, signal) { return this.request('GET', `${analysisPath(this.config,id)}/bff/reasoning-input`, { signal }); }
  reasoningHandoff(id, signal) { return this.request('GET', `${analysisPath(this.config,id)}/bff/handoff`, { signal }); }
  handoffReceipts(id, signal) { return this.request('GET', `${analysisPath(this.config,id)}/bff/handoff/receipts`, { signal }); }
  recordHandoffReceipt(id, receiptId, handoffSha256, kind, note = null) { return this.request('POST', `${analysisPath(this.config,id)}/bff/handoff/receipts/${receiptId}`, { body: { handoff_sha256: handoffSha256, kind, note } }); }
  runIntentSync(operationId, task) { return this.request('POST', `${intentPath(this.config,operationId)}/sync`, { body: task }); }
  runIntentAsync(operationId, task) { return this.request('POST', `${intentPath(this.config,operationId)}/async`, { body: task }); }
  runSync(operationId, task) { return this.request('POST', `${operationPath(this.config,operationId)}/sync`, { body: task }); }
  runReanalysisSync(operationId, priorAnalysisId, newAnalysisId, goal) { return this.runSync(operationId, { kind: 'REANALYZE', target_analysis_id: priorAnalysisId, parent_operation_id: null, idempotency_key: `ui-reanalysis-${operationId}`, input: { new_analysis_id: newAnalysisId, goal }, deadline_at: null, step_budget: 32 }); }
  runAsync(operationId, task) { return this.request('POST', `${operationPath(this.config,operationId)}/async`, { body: task }); }
  operation(operationId, etag) { return this.request('GET', operationPath(this.config,operationId), { etag }); }
  waitOperation(operationId, afterVersion, timeoutMs = 5000) { return this.request('GET', `${operationPath(this.config,operationId)}/wait?after_version=${afterVersion}&timeout_ms=${timeoutMs}`); }
  observeOperation(operationId, afterVersion = 0, afterEvent = 0, timeoutMs = 5000, limit = 100, signal) { return this.request('GET', `${operationPath(this.config,operationId)}/observe?after_version=${afterVersion}&after_event=${afterEvent}&timeout_ms=${timeoutMs}&limit=${limit}`, { signal }); }
  agentic(operationId, signal) { return this.request('GET', `${operationPath(this.config,operationId)}/agentic`, { signal }); }
  planAgentic(operationId) { return this.request('POST', `${operationPath(this.config,operationId)}/agentic`); }
  respondAgentic(operationId, stepId, body) { return this.request('POST', `${operationPath(this.config,operationId)}/agentic/steps/${stepId}/human-response`, { body }); }
  operationEvents(operationId, after = 0, limit = 50) { return this.request('GET', `${operationPath(this.config,operationId)}/events?after=${after}&limit=${limit}`); }
  cancelOperation(operationId, expectedVersion, reason) { return this.request('POST', `${operationPath(this.config,operationId)}/cancel`, { body: { expected_version: expectedVersion, reason } }); }
}
