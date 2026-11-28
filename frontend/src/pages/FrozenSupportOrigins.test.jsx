import { expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { FrozenSupportOrigins } from './FrozenSupportOrigins.jsx';

it('renders the recorded origin and counts distinct routes for the frozen claim', () => {
  const route = { argument_id: 'argument', grounding_set_id: 'route' };
  const other = { argument_id: 'argument', grounding_set_id: 'other' };
  const basis = {
    routes: [{ ...route, conclusion_text: 'The sighting time' }],
    shared_artifact_origins: [{
      conclusion_object_id: 'sighting',
      source_representation_id: 'origin',
      source_artifact_label: "Claire's interview",
      source_scope_locator: 'Second answer',
      derivations: [{ routes: [route, other] }, { routes: [route] }],
    }],
  };
  expect(renderToStaticMarkup(<FrozenSupportOrigins basis={basis}/>)).toContain(
    '2 supporting routes trace to Claire&#x27;s interview · Second answer.',
  );
  expect(renderToStaticMarkup(<FrozenSupportOrigins basis={basis}/>)).toContain('The sighting time');
  expect(renderToStaticMarkup(<FrozenSupportOrigins basis={{routes: []}}/>)).toBe('');
  expect(renderToStaticMarkup(<FrozenSupportOrigins basis={{shared_artifact_origins_truncated: true}}/>))
    .toContain('The analysis received a limited set of recorded derivations.');
});
