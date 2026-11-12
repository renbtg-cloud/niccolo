import { ApiError, NiccoloClient } from '../api/client.js';

export async function validateConnection(credentials, fetchImpl = fetch) {
  const client = new NiccoloClient(credentials, fetchImpl);
  // Workspace-level read is deliberate: a prepared Situation may not exist yet.
  // Connecting must validate the secret/workspace pair without forcing Situation creation.
  await client.sources(null, 1);
  return true;
}

export function connectionFailureMessage(error) {
  if (error instanceof ApiError) {
    if (error.status === 401) return 'Bearer token rejected. Check the secret and try again.';
    if (error.status === 403) return 'This bearer token does not grant read access to the prepared workspace.';
    if (error.status === 404) return 'Prepared workspace is unavailable to this session.';
    return `Niccolò rejected the connection (HTTP ${error.status}): ${error.message}`;
  }
  return 'Niccolò could not validate this session. Check that the local service is running and try again.';
}
