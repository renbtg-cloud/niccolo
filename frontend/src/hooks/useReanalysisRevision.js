import { useQuery } from '@tanstack/react-query';
import { ApiError } from '../api/client.js';
import { qk } from '../api/keys.js';
import { useClient } from './useClient.js';

export function useReanalysisRevision(analysisId) {
  const client = useClient();
  return useQuery({
    queryKey: client && analysisId ? qk.analysis(client.config, analysisId, 'reanalysis-revision') : ['reanalysis-revision','none'],
    enabled: Boolean(client && analysisId),
    queryFn: async () => {
      try {
        return (await client.reanalysisRevision(analysisId)).value;
      } catch (error) {
        if (error instanceof ApiError && error.status === 404) return null;
        throw error;
      }
    },
    retry: false,
  });
}
