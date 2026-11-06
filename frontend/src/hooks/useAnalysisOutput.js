import { useQuery } from '@tanstack/react-query';
import { qk } from '../api/keys.js';
import { ApiError } from '../api/client.js';
import { useClient } from './useClient.js';

export function useAnalysisOutput(analysisId, operation = null) {
  const client = useClient();
  return useQuery({
    queryKey: client && analysisId ? qk.analysis(client.config, analysisId, 'output') : ['analysis-output', 'none'],
    enabled: Boolean(client && analysisId),
    queryFn: async () => {
      try {
        return (await client.analysisOutput(analysisId)).value;
      } catch (error) {
        if (error instanceof ApiError && error.status === 404) return null;
        throw error;
      }
    },
    refetchInterval: (query) => {
      if (query.state.data) return false;
      if (!operation || operation.terminal) return false;
      return 500;
    },
    retry: false,
  });
}
