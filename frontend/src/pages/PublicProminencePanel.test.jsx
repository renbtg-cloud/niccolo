import { expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { PublicProminenceRecordForm, PublicProminenceReports } from './PublicProminencePanel.jsx';

const query = {
  subject_actor_id: 'frank',
  audience_actor_id: 'class',
  cue: 'FACE',
  on: '2026-11-19',
  known_through: '2026-11-20',
};
const actors = [
  { actor_id: 'frank', display_label: 'Frank' },
  { actor_id: 'class', display_label: 'Evening class' },
];
const provenance = [{
  object_id: 'report',
  representation_id: 'excerpt',
  artifact_label: "Mara's interview",
  scope_locator: 'Second answer',
}];

it('keeps the audience, report scope, availability and source visible', () => {
  const result = { query, coverage: 'RECORDED_REPORTS', truncated: true, records: [{
    record_id: 'record',
    cue: 'FACE',
    observation: 'EXPOSURE_REPORTED',
    audience_scope: 'Mara only',
    effective_from: '2026-11-19',
    effective_through: '2026-11-19',
    available_on: '2026-11-20',
    claim_object_id: 'report',
    representation_id: 'excerpt',
  }] };
  const html = renderToStaticMarkup(<PublicProminenceReports result={result} actors={actors} provenance={provenance}/>);
  expect(html).toContain('Evening class · Face');
  expect(html).toContain('reports available through 2026-11-20');
  expect(html).toContain('Exposure reported');
  expect(html).toContain('Mara only');
  expect(html).toContain('href="#source-report-excerpt"');
  expect(html).toContain('Second answer');
  expect(html).toContain('first 100 matching reports');
});

it('renders missing reports separately from a report of non-recognition', () => {
  const html = renderToStaticMarkup(<PublicProminenceReports
    result={{query, coverage: 'NO_RECORDED_REPORTS', records: []}} actors={actors} provenance={provenance}/>);
  expect(html).toContain('No reports recorded for this selection.');
  expect(html).not.toContain('Non-recognition reported');
});

it('only offers source passages already linked to a proposition', () => {
  const nodes = [
    { object_id: 'report', kind: 'PROPOSITION', text: 'Mara reports seeing the photo' },
    { object_id: 'guess', kind: 'HYPOTHESIS', text: 'Frank drove' },
    { object_id: 'unlinked', kind: 'PROPOSITION', text: 'Unlinked proposition' },
  ];
  const links = [...provenance, {
    object_id: 'guess', representation_id: 'other', artifact_label: 'Guess', scope_locator: 'Page 1',
  }];
  const html = renderToStaticMarkup(<QueryClientProvider client={new QueryClient()}>
    <PublicProminenceRecordForm client={{}} actors={actors} nodes={nodes} provenance={links}/>
  </QueryClientProvider>);
  expect(html).toContain('Report available on');
  expect(html).toContain('value="report:excerpt"');
  expect(html).toContain('Mara reports seeing the photo');
  expect(html).not.toContain('value="guess:other"');
  expect(html).not.toContain('Unlinked proposition');
});
