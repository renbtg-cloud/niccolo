import { useQuery } from '@tanstack/react-query';
import { qk } from '../api/keys.js';
import { useClient } from './useClient.js';

const PAGE_LIMIT = 200;
const MAX_PAGES_PER_REFRESH = 50;

async function loadOperationCheckpoints(client, operationId) {
  const checkpoints = [];
  let after = 0;
  let nextAfter = null;

  for (let page = 0; page < MAX_PAGES_PER_REFRESH; page += 1) {
    const value = (await client.operationCheckpoints(operationId, after, PAGE_LIMIT)).value;
    const additions = (value.checkpoints ?? [])
      .filter((checkpoint) => checkpoint.checkpoint_seq > after);
    checkpoints.push(...additions);
    nextAfter = value.next_after ?? null;

    if (!nextAfter) {
      return { checkpoints, next_after: null, catchup_bounded: false };
    }
    if (nextAfter <= after) {
      return { checkpoints, next_after: nextAfter, catchup_bounded: true };
    }
    after = nextAfter;
  }

  return {
    checkpoints,
    next_after: nextAfter,
    catchup_bounded: Boolean(nextAfter),
  };
}

export function useOperationCheckpoints(operationId) {
  const client = useClient();
  return useQuery({
    queryKey: client && operationId
      ? qk.operationCheckpoints(client.config, operationId)
      : ['operation-checkpoints','none'],
    enabled: Boolean(client && operationId),
    queryFn: async () => loadOperationCheckpoints(client, operationId),
    refetchInterval: (query) => {
      const checkpoints = query.state.data?.checkpoints ?? [];
      const last = checkpoints.at(-1);
      return last?.terminal && !query.state.data?.catchup_bounded ? false : 1500;
    },
    retry: 2,
  });
}
