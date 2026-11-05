const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

export class BackendError extends Error {
  constructor(status, message) {
    super(message);
    this.name = "BackendError";
    this.status = status;
  }
}

export function backendConfig(workspace, situation, token) {
  const w = workspace.trim();
  const s = situation.trim();
  const t = token.trim();
  if (!UUID.test(w) || !UUID.test(s) || !t) throw new Error("Workspace, Situation UUID and bearer token are required.");
  return Object.freeze({ workspace: w, situation: s, token: t });
}

export function situationPath(config) {
  return `/v1/workspaces/${config.workspace}/situations/${config.situation}`;
}

async function request(fetchImpl, config, method, path, body) {
  const headers = { "Authorization": `Bearer ${config.token}`, "Accept": "application/json" };
  if (body !== undefined) headers["Content-Type"] = "application/json";
  const response = await fetchImpl(path, {
    method,
    headers,
    cache: "no-store",
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const value = await response.json().catch(() => ({}));
  if (!response.ok) throw new BackendError(response.status, value.error ?? `HTTP ${response.status}`);
  return value;
}

export function openSituation(fetchImpl, config) {
  return request(fetchImpl, config, "GET", situationPath(config));
}
export function createSituation(fetchImpl, config, focus) {
  return request(fetchImpl, config, "POST", situationPath(config), { focus });
}
export function setPersistentFocus(fetchImpl, config, focus) {
  return request(fetchImpl, config, "POST", `${situationPath(config)}/focus`, { focus });
}
export function addPersistentContribution(fetchImpl, config, id, text, hypothetical) {
  return request(fetchImpl, config, "POST", `${situationPath(config)}/contributions/${id}`, {
    kind: hypothetical ? "hypothetical" : "user_reported",
    text,
  });
}
export function retractPersistentContribution(fetchImpl, config, id) {
  return request(fetchImpl, config, "POST", `${situationPath(config)}/contributions/${id}/retract`);
}
export function listPersistentRevisions(fetchImpl, config) {
  return request(fetchImpl, config, "GET", `${situationPath(config)}/revisions?after=0&limit=100`);
}
