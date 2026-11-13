import { useQuery, useQueryClient } from '@tanstack/react-query';
import { qk } from '../api/keys.js';
import { useClient } from './useClient.js';
import { completedAnalysisOutputRefreshId, eventPageNeedsCatchup, mergeOperationObservation, operationPhase, observationPollDelay } from '../state/operationObservation.js';

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
      const incoming = (
        await client.observeOperation(
          operationId,
          afterVersion,
          afterEvent,
          5000,
          OBSERVATION_EVENT_LIMIT,
        )
      ).value;
      const merged = mergeOperationObservation(previous, incoming);
      const observed = merged
        ? {
            ...merged,
            eventsCatchupPending: eventPageNeedsCatchup(incoming, OBSERVATION_EVENT_LIMIT),
          }
        : merged;
      if (observed?.operation) queryClient.setQueryData(qk.operation(client.config, operationId), observed.operation);
      if (observed?.events) queryClient.setQueryData(qk.operationEvents(client.config, operationId), observed.events);
      const completedAnalysis = completedAnalysisOutputRefreshId(previous, observed);
      if (completedAnalysis) {
        await queryClient.invalidateQueries({
          queryKey: qk.analysis(client.config, completedAnalysis, 'output'),
        });
      }
      return observed;
    },
    refetchInterval: (q) => {
      if (q.state.data?.eventsCatchupPending) return 50;
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
    eventsCatchupPending: query.data?.eventsCatchupPending ?? false,
    cursor: query.data?.cursor ?? 0,
    phase: operationPhase(query.data, { pending: query.isPending, fetching: query.isFetching, error: query.error }),
    stale: query.isStale,
  };
}
