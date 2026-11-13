import { useRef } from 'react';
import { useQuery } from '@tanstack/react-query';
import { qk } from '../api/keys.js';
import { useClient } from './useClient.js';

const PAGE_SIZE = 200;
const MAX_PAGES_PER_REFRESH = 50;

export function useOperationCheckpoints(operationId) {
  const client = useClient();
  const accumulated = useRef({ key: null, checkpoints: [] });
  return useQuery({
    queryKey: client && operationId
      ? qk.operationCheckpoints(client.config, operationId)
      : ['operation-checkpoints','none'],
    enabled: Boolean(client && operationId),
    queryFn: async () => {
      const key = JSON.stringify([
        client.config.workspace,
        client.config.situation,
        operationId,
      ]);
      if (accumulated.current.key !== key) {
        accumulated.current = { key, checkpoints: [] };
      }
      const checkpoints = accumulated.current.checkpoints;
      let after = checkpoints.at(-1)?.checkpoint_seq ?? 0;
      let nextAfter = null;
      for (let pageNo = 0; pageNo < MAX_PAGES_PER_REFRESH; pageNo += 1) {
        const page = (await client.operationCheckpoints(operationId, after, PAGE_SIZE)).value;
        const additions = (page.checkpoints ?? []).filter(
          checkpoint => checkpoint.checkpoint_seq > after,
        );
        checkpoints.push(...additions);
        nextAfter = page.next_after ?? null;
        if (!nextAfter) break;
        after = nextAfter;
      }
      return {
        checkpoints: [...checkpoints],
        next_after: nextAfter,
        catchup_bounded: Boolean(nextAfter),
      };
    },
    refetchInterval: (query) => {
      const data = query.state.data;
      if (data?.catchup_bounded) return 0;
      const last = data?.checkpoints?.at(-1);
      return last?.terminal ? false : 1500;
    },
    retry: 2,
  });
}
