export class ApiError extends Error { constructor(status, message) { super(message); this.status = status; } }
export function situationPath(c) { return `/v1/workspaces/${c.workspace}/situations/${c.situation}`; }
export function sourcesPath(c) { return `/v1/workspaces/${c.workspace}/sources`; }
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
  situation() { return this.request('GET', situationPath(this.config)); }
  createSituation(focus) { return this.request('POST', situationPath(this.config), { body: { focus } }); }
  setFocus(focus) { return this.request('POST', `${situationPath(this.config)}/focus`, { body: { focus } }); }
  sources(after = 0, limit = 100) { return this.request('GET', `${sourcesPath(this.config)}?after=${after}&limit=${limit}`); }
  ingestSource(documentId, { locator, contentType, body }) { return this.requestRaw('POST', `${sourcesPath(this.config)}/${documentId}`, { body, contentType, locator }); }
  addSituationSource(documentId) { return this.request('POST', `${situationPath(this.config)}/sources/${documentId}`); }
  revisions() { return this.request('GET', `${situationPath(this.config)}/revisions?after=0&limit=100`); }
  analysisReview(id) { return this.request('GET', `${analysisPath(this.config,id)}/review`); }
  analysisCoverage(id) { return this.request('GET', `${analysisPath(this.config,id)}/coverage`); }
  analysisContext(id) { return this.request('GET', `${analysisPath(this.config,id)}/context-usage`); }
  analysisDelta(id) { return this.request('GET', `${analysisPath(this.config,id)}/delta`); }
  analysisOutput(id) { return this.request('GET', `${analysisPath(this.config,id)}/output`); }
  dashboard(id, operationId) { const q = operationId ? `?operation=${operationId}` : ''; return this.request('GET', `${analysisPath(this.config,id)}/bff/dashboard${q}`); }
  epistemic(id) { return this.request('GET', `${analysisPath(this.config,id)}/bff/epistemic`); }
  runIntentSync(operationId, task) { return this.request('POST', `${intentPath(this.config,operationId)}/sync`, { body: task }); }
  runIntentAsync(operationId, task) { return this.request('POST', `${intentPath(this.config,operationId)}/async`, { body: task }); }
  runSync(operationId, task) { return this.request('POST', `${operationPath(this.config,operationId)}/sync`, { body: task }); }
  runAsync(operationId, task) { return this.request('POST', `${operationPath(this.config,operationId)}/async`, { body: task }); }
  operation(operationId, etag) { return this.request('GET', operationPath(this.config,operationId), { etag }); }
  waitOperation(operationId, afterVersion, timeoutMs = 5000) { return this.request('GET', `${operationPath(this.config,operationId)}/wait?after_version=${afterVersion}&timeout_ms=${timeoutMs}`); }
  observeOperation(operationId, afterVersion = 0, afterEvent = 0, timeoutMs = 5000, limit = 100) { return this.request('GET', `${operationPath(this.config,operationId)}/observe?after_version=${afterVersion}&after_event=${afterEvent}&timeout_ms=${timeoutMs}&limit=${limit}`); }
  agentic(operationId) { return this.request('GET', `${operationPath(this.config,operationId)}/agentic`); }
  planAgentic(operationId) { return this.request('POST', `${operationPath(this.config,operationId)}/agentic`); }
  respondAgentic(operationId, stepId, body) { return this.request('POST', `${operationPath(this.config,operationId)}/agentic/steps/${stepId}/human-response`, { body }); }
  operationEvents(operationId, after = 0, limit = 50) { return this.request('GET', `${operationPath(this.config,operationId)}/events?after=${after}&limit=${limit}`); }
  cancelOperation(operationId, expectedVersion, reason) { return this.request('POST', `${operationPath(this.config,operationId)}/cancel`, { body: { expected_version: expectedVersion, reason } }); }
}
