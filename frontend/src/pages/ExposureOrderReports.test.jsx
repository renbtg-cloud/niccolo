import { expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ExposureOrderReportForm, ExposureOrderReports } from './ExposureOrderReports.jsx';

it('shows both attributed directions, their source links, and the date comparison separately', () => {
  const report = (id, direction) => ({
    report_id: id, direction, source: { object_id: id, representation_id: `passage-${id}` },
    available_on: '2026-11-25', recorded_by: 'investigator',
  });
  const result = {
    date_order: 'EXPOSURE_BEFORE_TESTIMONY', order_reports_truncated: true,
    order_reports: [
      report('mara', 'EXPOSURE_BEFORE_TESTIMONY'),
      report('june', 'EXPOSURE_AFTER_TESTIMONY'),
    ],
  };
  const provenance = [
    { object_id: 'mara', representation_id: 'passage-mara', artifact_label: 'Mara interview', scope_locator: 'Answer 2' },
    { object_id: 'june', representation_id: 'passage-june', artifact_label: 'June interview', scope_locator: 'Answer 4' },
  ];
  const html = renderToStaticMarkup(<ExposureOrderReports result={result} provenance={provenance}/>);
  expect(html).toContain('By calendar dates · Exposure before testimony');
  expect(html).toContain('Exposure after testimony');
  expect(html).toContain('href="#source-mara-passage-mara"');
  expect(html).toContain('June interview · Answer 4');
  expect(html).toContain('available 2026-11-25');
  expect(html).toContain('at most 50 reports in each direction');
});

it('asks for the source passage, relative order, and availability date', () => {
  const html = renderToStaticMarkup(<QueryClientProvider client={new QueryClient()}>
    <ExposureOrderReportForm sources={new Map()}
      sourceOptions={<option value="mara:passage">Follow-up with Mara</option>}/>
  </QueryClientProvider>);
  expect(html).toContain('Passage that states the order');
  expect(html).toContain('Follow-up with Mara');
  expect(html).toContain('Report available on');
  expect(html).toContain('value="EXPOSURE_BEFORE_TESTIMONY"');
  expect(html).not.toContain('type="time"');
});
