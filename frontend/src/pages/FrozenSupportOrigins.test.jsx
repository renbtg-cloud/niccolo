import { expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { FrozenArtifactCoverage, FrozenSupportOrigins, PublishedSupportOrigins } from './FrozenSupportOrigins.jsx';

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

it('renders artifact input coverage and indicates omissions independently of legacy fragments', () => {
  const input = {
    artifact_representation_count_total: 3,
    artifact_representation_count_included: 2,
    artifact_representation_bytes_included: 64,
    artifact_packet_truncated: true,
    source_packet_truncated: false,
  };
  const markup = renderToStaticMarkup(<FrozenArtifactCoverage input={input}/>);
  expect(markup).toContain('<strong>2/3</strong> attributed artifact representations included');
  expect(markup).toContain('64 text bytes');
  expect(markup).toContain('Some frozen artifact material exceeded the packet limits');
  expect(renderToStaticMarkup(<FrozenArtifactCoverage input={{source_packet_truncated: true}}/>)).toBe('');
});

it('shows a common origin beside the answer when a published claim cited multiple routes from it', () => {
  const owen = { argument_id: 'argument', grounding_set_id: 'owen' };
  const ruth = { argument_id: 'argument', grounding_set_id: 'ruth' };
  const other = { argument_id: 'argument', grounding_set_id: 'other' };
  const basis = { shared_artifact_origins: [{
    conclusion_object_id: 'sighting',
    source_representation_id: 'origin',
    source_artifact_label: 'Claire',
    source_scope_locator: 'Second answer',
    derivations: [{ routes: [owen] }, { routes: [ruth] }],
  }] };
  const render = (citations) => renderToStaticMarkup(<PublishedSupportOrigins basis={basis} citations={citations}/>);
  const citation = (route, claim_id = 'sighting') => ({ ...route, claim_id });
  expect(render([citation(owen), citation(ruth)])).toContain('same claim trace to Claire · Second answer');
  expect(render([citation(owen), citation(owen), citation(other)])).toBe('');
  expect(render([citation(owen), citation(ruth, 'dossier')])).toBe('');
});
