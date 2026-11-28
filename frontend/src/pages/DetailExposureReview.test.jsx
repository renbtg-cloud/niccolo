import { expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { DetailExposureResult } from './DetailExposureReview.jsx';

const nodes = [
  { object_id: 'hat', text: 'Mara recalls a red hat.' },
  { object_id: 'driver', text: 'Frank drove the tow truck.' },
  { object_id: 'truck', text: 'Mara saw a tow truck.' },
];
const testimony_detail = { object_id: 'hat', representation_id: 'interview-detail' };

it('renders only the detail and affected route returned by the review', () => {
  const result = {
    disposition: 'REVIEW_CANDIDATE',
    testimony_detail,
    affected_routes: [{ argument_id: 'argument', grounding_set_id: 'route', conclusion_object_id: 'driver' }],
  };
  const html = renderToStaticMarkup(<DetailExposureResult result={result} nodes={nodes}/>);
  expect(html).toContain('makes this detail worth reviewing');
  expect(html).toContain('Mara recalls a red hat.');
  expect(html).toContain('href="#workbench-object-driver"');
  expect(html).not.toContain('Mara saw a tow truck.');
});

it.each([
  ['EXPOSURE_AFTER_TESTIMONY', 'follows the testimony'],
  ['ORDER_UNCERTAIN', 'leave the order uncertain'],
  ['EXPOSURE_NOT_AVAILABLE_AT_CUTOFF', 'unavailable at the chosen cutoff'],
])('preserves the backend disposition %s', (disposition, message) => {
  const result = {
    disposition, testimony_detail, affected_routes: [], needs_finer_timing: disposition === 'ORDER_UNCERTAIN',
  };
  const html = renderToStaticMarkup(<DetailExposureResult result={result} nodes={nodes}/>);
  expect(html).toContain(message);
  expect(html).not.toContain('Routes that depend on this detail');
  expect(html.includes('more precise date')).toBe(result.needs_finer_timing);
});
