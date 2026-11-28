import { expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { DetailExposureResult, DetailExposureRouteDecisions } from './DetailExposureReview.jsx';

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
  ['ORDER_UNCERTAIN', 'leaves the order uncertain'],
  ['ORDER_CONFLICT', 'disagree about which came first'],
  ['EXPOSURE_NOT_AVAILABLE_AT_CUTOFF', 'unavailable at the chosen cutoff'],
])('preserves the backend disposition %s', (disposition, message) => {
  const result = {
    disposition, testimony_detail, affected_routes: [], needs_finer_timing: disposition === 'ORDER_UNCERTAIN',
  };
  const html = renderToStaticMarkup(<DetailExposureResult result={result} nodes={nodes}/>);
  expect(html).toContain(message);
  expect(html).not.toContain('Routes that depend on this detail');
  expect(html.includes('source that establishes which came first')).toBe(result.needs_finer_timing);
});

it('allows an explicit use decision only for an affected supporting route', () => {
  const makeRoute = (id, effect, rationale) => ({
    argument_id: 'argument', grounding_set_id: id, effect, rationale,
  });
  const affectedRoute = makeRoute('hat-route', 'SUPPORTS', 'Route relying on the red hat');
  const opposition = makeRoute('opposition', 'OPPOSES', 'Opposing route');
  const independent = makeRoute('independent', 'SUPPORTS', "Jules's independent route");
  const history = [{
    event_id: 'prior',
    route: affectedRoute,
    disposition: 'SUSPEND',
    rationale: 'Verify the newspaper archive first.',
  }];
  const render = (needsReview) => renderToStaticMarkup(<QueryClientProvider client={new QueryClient()}>
    <DetailExposureRouteDecisions
      result={{ detail_needs_review: needsReview, affected_routes: [affectedRoute, opposition] }}
      groundingSets={[affectedRoute, opposition, independent]} routeHistory={history}/>
  </QueryClientProvider>);
  const html = render(true);
  expect(html).toContain('Route relying on the red hat');
  expect(html).toContain('Restore use');
  expect(html).toContain('Verify the newspaper archive first.');
  expect(html).not.toContain('Opposing route');
  expect(html).not.toContain('independent route');
  expect(render(false)).toBe('');
});

it.each(['ORDER_CONFLICT', 'ORDER_UNCERTAIN'])('offers to revisit a prior suspension in %s', (disposition) => {
  const route = {
    argument_id: 'argument', grounding_set_id: 'hat-route', effect: 'SUPPORTS', rationale: 'Red hat detail',
  };
  const unrelated = { ...route, grounding_set_id: 'unrelated', rationale: 'Separate sighting' };
  const html = renderToStaticMarkup(<QueryClientProvider client={new QueryClient()}>
    <DetailExposureRouteDecisions
      result={{ disposition, reconsiderable_suspensions: [route] }}
      groundingSets={[route, unrelated]} routeHistory={[]}/>
  </QueryClientProvider>);
  expect(html).toContain('Reconsider an earlier suspension');
  expect(html).toContain('Restore use');
  expect(html).toContain('Red hat detail');
  expect(html).not.toContain('Suspend use');
  expect(html).not.toContain('Separate sighting');
});
