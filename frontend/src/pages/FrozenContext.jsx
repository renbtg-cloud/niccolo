import { useState } from 'react';
import { useInfiniteQuery } from '@tanstack/react-query';
import { qk } from '../api/keys.js';

const kindLabels = {
  USER_REPORTED: 'Human report',
  HYPOTHETICAL: 'What-if assumption',
  INVESTIGATIVE_PREFERENCE: 'Investigative preference',
};

export function FrozenContext({ client, analysis }) {
  const [open, setOpen] = useState(false);
  const context = useInfiniteQuery({
    queryKey: qk.analysis(client.config, analysis, 'frozen-context'),
    enabled: open,
    initialPageParam: null,
    queryFn: async ({ pageParam, signal }) => (await client.analysisContext(analysis, signal, pageParam)).value,
    getNextPageParam: page => page.next_after ?? undefined,
  });
  const contributions = context.data?.pages.flatMap(page => page.contributions) ?? [];
  return <details className="review-panel" onToggle={event=>setOpen(event.currentTarget.open)}>
    <summary>Read the human context in this analysis</summary>
    {context.isLoading && <p role="status">Loading frozen human context…</p>}
    {contributions.length > 0 && <ul>{contributions.map(item=><li key={item.contribution_id}>
      <p><strong>{kindLabels[item.kind] ?? 'Human contribution'}</strong> · {item.used_by_analysis ? 'Recorded as used' : 'Present in input'}</p>
      <p className="answer-text">{item.text}</p>
    </li>)}</ul>}
    {context.isError && <p role="alert">Could not load human context: {context.error.message}</p>}
    {!context.data && context.isError && <button type="button" onClick={()=>void context.refetch()}>Retry human context</button>}
    {context.hasNextPage && <button type="button" disabled={context.isFetching} onClick={()=>void context.fetchNextPage()}>
      {context.isFetchingNextPage ? 'Loading…' : context.isFetchNextPageError ? 'Retry next page' : 'More human context'}
    </button>}
  </details>;
}
