import { useQuery } from '@tanstack/react-query';

export function useRuntimeReadiness(client) {
  return useQuery({
    queryKey: ['mvp-runtime-readiness'],
    enabled: Boolean(client),
    queryFn: async ({ signal }) => (await client.readiness(signal)).value,
    staleTime: 60000,
    retry: false,
  });
}
