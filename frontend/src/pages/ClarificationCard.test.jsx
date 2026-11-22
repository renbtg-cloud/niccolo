import { expect, it, vi } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderToStaticMarkup } from 'react-dom/server';
import { qk } from '../api/keys.js';
import { ClarificationCard } from './ClarificationCard.jsx';

const config = vi.hoisted(() => ({ workspace: 'workspace', situation: 'situation' }));
vi.mock('../hooks/useClient.js', () => ({ useClient: () => ({ config }) }));
vi.mock('../stores/ui.js', () => ({ useUiStore: (select) => select({ analysis: 'analysis' }) }));

it('renders recorded answers and pagination without losing the editable draft', () => {
  const cache = new QueryClient();
  const question = { question_id: 'question', text: 'Who collected it?', reason: 'Collection gap', input_revision: 2 };
  cache.setQueryData([...qk.analysis(config, 'analysis', 'clarification-answers'), 'question', null], {
    answers: [{ answer_id: 'answer', text: 'Alice collected it.', kind: 'USER_REPORTED' }],
    next_after: 'answer',
  });
  const markup = renderToStaticMarkup(
    <QueryClientProvider client={cache}>
      <ClarificationCard question={question} draft="Check with Bob" onChange={() => {}} onSubmit={() => {}} saving={false}/>
    </QueryClientProvider>,
  );
  expect(markup).toContain('Who collected it?');
  expect(markup).toContain('Alice collected it.');
  expect(markup).toContain('More answers');
  expect(markup).toContain('Check with Bob');
  expect(markup).not.toContain('Previous answers');
});

it('links a rejected multiline draft to its visible validation error', () => {
  const question = { question_id: 'question', text: 'What changed?' };
  const markup = renderToStaticMarkup(
    <QueryClientProvider client={new QueryClient()}>
      <ClarificationCard question={question} draft={'First\nSecond'} onChange={()=>{}} onSubmit={()=>{}} saving={false}/>
    </QueryClientProvider>,
  );
  const description = markup.match(/aria-describedby="([^"]+)"/)?.[1];
  expect(description).toBeTruthy();
  expect(markup).toContain(`id="${description}" role="alert"`);
  expect(markup).toContain('aria-invalid="true"');
  expect(markup).toContain('<button type="submit" disabled="">Add answer as context');
});
