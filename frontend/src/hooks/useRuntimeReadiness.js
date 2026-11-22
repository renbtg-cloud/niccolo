import { useQuery } from '@tanstack/react-query';

export function useRuntimeReadiness(client) {
  return useQuery({
    queryKey: ['mvp-runtime-readiness'],
    enabled: Boolean(client),
    queryFn: async () => (await client.readiness()).value,
    staleTime: 60000,
    retry: false,
  });
}
