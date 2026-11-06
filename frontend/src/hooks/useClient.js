import { useMemo } from 'react';
import { useSessionStore } from '../stores/session.js';
import { NiccoloClient } from '../api/client.js';
export function useClient() {
  const config = useSessionStore((s) => ({ workspace:s.workspace, situation:s.situation, token:s.token, connected:s.connected }));
  return useMemo(() => config.connected ? new NiccoloClient(config) : null, [config.workspace, config.situation, config.token, config.connected]);
}
