import { expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { DraftGraphPreview, WorkbenchGraph } from './WorkbenchGraph.jsx';

it('renders an incoming link in its recorded direction even when the focus is its destination', () => {
  const nodes = [
    { object_id: 'claim', kind: 'PROPOSITION', text: 'A witness reports a ride request.' },
    { object_id: 'question', kind: 'QUESTION', text: 'Did the victim get in the car?' },
  ];
  const relations = [
    { relation_id: 'incoming', kind: 'PRESUPPOSES', left_object_id: 'question', right_object_id: 'claim', rationale: 'The question assumes the context.' },
  ];
  const markup = renderToStaticMarkup(<WorkbenchGraph nodes={nodes} relations={relations}/>);
  const link = markup.slice(markup.indexOf('Connections to the focused object'));
  expect(link).toMatch(/Did the victim get in the car\?[^]*presupposes[^]*→[^]*A witness reports a ride request\./);
  expect(link).toContain('The question assumes the context.');
});

it('shows a symmetric draft connection with both endpoints and the edited rationale', () => {
  const graph = {
    objects: [
      { kind: 'PROPOSITION', text: 'Witness A reports a ride.' },
      { kind: 'PROPOSITION', text: 'Witness B reports no ride.' },
    ],
    relations: [{ kind: 'CONTRADICTS', from: 0, to: 1, explanation: 'Accounts conflict.' }],
  };
  const markup = renderToStaticMarkup(<DraftGraphPreview graph={graph}/>);
  expect(markup).toMatch(/Witness A reports a ride\.[^]*contradicts[^]*↔[^]*Accounts conflict\.[^]*Witness B reports no ride\./);
});
