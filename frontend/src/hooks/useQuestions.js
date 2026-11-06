import { useQuery } from '@tanstack/react-query';
import { useClient } from './useClient.js';

export function useQuestions() {
  const client = useClient();
  return useQuery({
    queryKey: client ? ['questions', client.config.workspace] : ['questions','none'],
    enabled: Boolean(client),
    queryFn: async () => (await client.questions(null, 100)).value,
  });
}
