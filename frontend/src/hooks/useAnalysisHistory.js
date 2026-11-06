import { useQuery } from '@tanstack/react-query';
import { qk } from '../api/keys.js';
import { useClient } from './useClient.js';

export function useAnalysisHistory() {
  const client = useClient();
  return useQuery({
    queryKey: client ? qk.analysisHistory(client.config) : ['analysis-history','none'],
    enabled: Boolean(client),
    queryFn: async () => (await client.analysisHistory(null, 100)).value,
  });
}
