import { useMemo } from 'react';
import { useSessionStore } from '../stores/session.js';
import { NiccoloClient } from '../api/client.js';
export function useClient() {
  // Zustand 5 requires stable selector snapshots. Allocating a fresh object in
  // the selector can create an endless React 19 render loop.
  const workspace = useSessionStore((s) => s.workspace);
  const situation = useSessionStore((s) => s.situation);
  const token = useSessionStore((s) => s.token);
  const connected = useSessionStore((s) => s.connected);
  return useMemo(
    () => connected ? new NiccoloClient({ workspace, situation, token, connected }) : null,
    [workspace, situation, token, connected],
  );
}
