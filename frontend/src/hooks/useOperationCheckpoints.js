import { useQuery } from '@tanstack/react-query';
import { qk } from '../api/keys.js';
import { useClient } from './useClient.js';

export function useOperationCheckpoints(operationId) {
  const client = useClient();
  return useQuery({
    queryKey: client && operationId
      ? qk.operationCheckpoints(client.config, operationId)
      : ['operation-checkpoints','none'],
    enabled: Boolean(client && operationId),
    queryFn: async () => (await client.operationCheckpoints(operationId, 0, 200)).value,
    refetchInterval: (query) => {
      const checkpoints = query.state.data?.checkpoints ?? [];
      const last = checkpoints.at(-1);
      return last?.terminal ? false : 1500;
    },
    retry: 2,
  });
}
