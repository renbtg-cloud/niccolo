import { useQuery } from '@tanstack/react-query';
import { qk } from '../api/keys.js';
import { useClient } from './useClient.js';

const TERMINAL_OPERATION_STATES = new Set(['SUCCEEDED','FAILED','CANCELLED']);

export function agenticPlanPollInterval(query, operationState) {
  const view=query?.state?.data;
  if(view?.blocked || view?.waiting_for_human) return false;
  // A controller revision can be momentarily complete while the durable
  // operation is still RUNNING and about to install a human-attention plan.
  // Keep polling until the operation itself is terminal.
  if(TERMINAL_OPERATION_STATES.has(operationState)) return false;
  return 1000;
}

export function useAgenticPlan(operationId, operationState) {
  const client = useClient();
  return useQuery({
    queryKey: client && operationId ? qk.agentic(client.config, operationId) : ['agentic-plan','none'],
    enabled: Boolean(client && operationId),
    queryFn: async () => (await client.agentic(operationId)).value,
    retry: false,
    refetchInterval: (query) => agenticPlanPollInterval(query, operationState),
  });
}
