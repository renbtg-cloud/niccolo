import { useQuery } from '@tanstack/react-query';
import { qk } from '../api/keys.js';
import { useClient } from './useClient.js';

export function agenticPlanPollInterval(query) {
  const view=query?.state?.data;
  if(view?.complete || view?.blocked || view?.waiting_for_human) return false;
  return 1000;
}

export function useAgenticPlan(operationId) {
  const client = useClient();
  return useQuery({
    queryKey: client && operationId ? qk.agentic(client.config, operationId) : ['agentic-plan','none'],
    enabled: Boolean(client && operationId),
    queryFn: async () => (await client.agentic(operationId)).value,
    retry: false,
    refetchInterval: agenticPlanPollInterval,
  });
}
