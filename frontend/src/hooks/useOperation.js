import { useQuery } from '@tanstack/react-query';
import { qk } from '../api/keys.js';
import { useClient } from './useClient.js';
export function useOperation(operationId) {
  const client = useClient();
  return useQuery({
    queryKey: client && operationId ? qk.operation(client.config, operationId) : ['operation','none'],
    enabled: Boolean(client && operationId),
    queryFn: async () => (await client.operation(operationId)).value,
    refetchInterval: (query) => {
      const value=query.state.data; if (!value || value.terminal) return false;
      return value.retry_after_ms ?? 1000;
    },
  });
}
