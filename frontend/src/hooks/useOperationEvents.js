import { useQuery } from '@tanstack/react-query';
import { qk } from '../api/keys.js';
import { useClient } from './useClient.js';
export function useOperationEvents(operationId) {
  const client = useClient();
  return useQuery({
    queryKey: client && operationId ? qk.operationEvents(client.config, operationId) : ['operation-events','none'],
    enabled: Boolean(client && operationId),
    queryFn: async () => (await client.operationEvents(operationId, 0, 200)).value,
    refetchInterval: 1000,
  });
}
