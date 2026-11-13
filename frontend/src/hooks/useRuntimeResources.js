import { useQuery } from '@tanstack/react-query';
import { qk } from '../api/keys.js';
import { useClient } from './useClient.js';

export function useRuntimeResources() {
  const client = useClient();
  return useQuery({
    queryKey: client ? qk.runtimeResources(client.config) : ['runtime-resources','none'],
    enabled: Boolean(client),
    queryFn: async () => (await client.runtimeResources()).value,
    refetchInterval: 5000,
    staleTime: 2000,
    retry: 1,
  });
}
