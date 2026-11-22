import { useQuery } from '@tanstack/react-query';
import { qk } from '../api/keys.js';
import { useClient } from './useClient.js';

export function useAnalysisHistory(before = null) {
  const client = useClient();
  return useQuery({
    queryKey: client ? [...qk.analysisHistory(client.config), before] : ['analysis-history','none'],
    enabled: Boolean(client),
    queryFn: async ({ signal }) => (await client.analysisHistory(before, 100, signal)).value,
  });
}
