import { useQuery, useQueryClient } from '@tanstack/react-query';
import { qk } from '../api/keys.js';
import { useClient } from './useClient.js';
import { completedAnalysisOutputRefreshId, mergeOperationObservation, operationPhase, observationPollDelay } from '../state/operationObservation.js';

const OBSERVATION_EVENT_LIMIT = 100;

export function useOperationObserver(operationId) {
  const client = useClient();
  const queryClient = useQueryClient();
  const query = useQuery({
    queryKey: client && operationId ? qk.operationObservation(client.config, operationId) : ['operation-observation', 'none'],
    enabled: Boolean(client && operationId),
    queryFn: async ({ queryKey }) => {
      const previous = queryClient.getQueryData(queryKey) ?? null;
      const afterVersion = previous?.operation?.state_version ?? 0;
      const afterEvent = previous?.cursor ?? 0;
      const incoming = (await client.observeOperation(
        operationId,
        afterVersion,
        afterEvent,
        5000,
        OBSERVATION_EVENT_LIMIT,
      )).value;
      const merged = mergeOperationObservation(previous, {
        ...incoming,
        event_page_may_have_more: (incoming.events?.length ?? 0) === OBSERVATION_EVENT_LIMIT,
      });
      if (merged?.operation) queryClient.setQueryData(qk.operation(client.config, operationId), merged.operation);
      if (merged?.events) queryClient.setQueryData(qk.operationEvents(client.config, operationId), merged.events);
      const completedAnalysis = completedAnalysisOutputRefreshId(previous, merged);
      if (completedAnalysis) {
        await queryClient.invalidateQueries({
          queryKey: qk.analysis(client.config, completedAnalysis, 'output'),
        });
      }
      return merged;
    },
    refetchInterval: (q) => {
      if (q.state.data?.eventsMayRemain) return 25;
      const state = q.state.data?.operation?.state;
      return observationPollDelay(state);
    },
    staleTime: 0,
    retry: 2,
  });
  return {
    ...query,
    operation: query.data?.operation ?? null,
    events: query.data?.events ?? [],
    eventsTruncated: query.data?.eventsTruncated ?? false,
    cursor: query.data?.cursor ?? 0,
    phase: operationPhase(query.data, { pending: query.isPending, fetching: query.isFetching, error: query.error }),
    stale: query.isStale,
  };
}
