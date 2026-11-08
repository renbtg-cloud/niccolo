import { useQuery } from '@tanstack/react-query';
import { useClient } from './useClient.js';

export function useQuestions(after = null) {
  const client = useClient();
  return useQuery({
    queryKey: client ? ['questions', client.config.workspace, after] : ['questions','none'],
    enabled: Boolean(client),
    queryFn: async () => (await client.questions(after, 100)).value,
  });
}
