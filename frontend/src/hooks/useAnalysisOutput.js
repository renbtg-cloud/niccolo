import { useEffect } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { qk } from '../api/keys.js';
import { ApiError } from '../api/client.js';
import { useClient } from './useClient.js';

export function useAnalysisOutput(analysisId, operation = null) {
  const client = useClient();
  const queryClient = useQueryClient();
  const config = client?.config;
  const terminal = operation?.terminal === true;
  useEffect(() => {
    if (terminal && config && analysisId) {
      void queryClient.invalidateQueries({
        queryKey: qk.analysis(config, analysisId, 'output'),
        exact: true,
      });
    }
  }, [terminal, config, analysisId, queryClient]);
  return useQuery({
    queryKey: client && analysisId ? qk.analysis(client.config, analysisId, 'output') : ['analysis-output', 'none'],
    enabled: Boolean(client && analysisId),
    queryFn: async ({ signal }) => {
      try {
        return (await client.analysisOutput(analysisId, signal)).value;
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
