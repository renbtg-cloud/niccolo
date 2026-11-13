import { useQuery } from '@tanstack/react-query';
import { qk } from '../api/keys.js';
import { useClient } from './useClient.js';

export function usePurposeHistory() {
  const client = useClient();
  return useQuery({
    queryKey: client ? qk.purposeHistory(client.config) : ['purpose-history','none'],
    enabled: Boolean(client),
    queryFn: async () => (await client.purposeHistory()).value,
    retry: 1,
  });
}
