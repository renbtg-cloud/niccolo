import { useRef, useState } from 'react';

const symmetric = new Set(['EQUIVALENT_TO', 'CONTRADICTS', 'PARTIAL_OVERLAP']);
const humanize = (value) => value.toLowerCase().replaceAll('_', ' ');

function GraphObject({ object, onOpen }) {
  const content = <><small>{humanize(object.kind)}</small><span>{object.text}</span></>;
  return onOpen
    ? <button type="button" className="graph-object" onClick={() => onOpen(object.id)}>{content}</button>
    : <div className="graph-object">{content}</div>;
}

function GraphConnection({ edge, from, to, focused, onOpen }) {
  const direction = symmetric.has(edge.kind) ? '↔' : '→';
  return <li className="graph-connection">
    <GraphObject object={from} onOpen={from.id === focused ? undefined : onOpen}/>
    <div className="graph-link">
      <strong>{humanize(edge.kind)} <span aria-hidden="true">{direction}</span></strong>
      {edge.explanation && <small>{edge.explanation}</small>}
    </div>
    <GraphObject object={to} onOpen={to.id === focused ? undefined : onOpen}/>
  </li>;
}

export function DraftGraphPreview({ graph }) {
  const objects = graph.objects.map((object, id) => ({ ...object, id }));
  const drawable = graph.relations.filter(({ from, to }) => objects[from] && objects[to]);
  if (!drawable.length) return null;
  return <ul className="graph-connections" aria-label="Proposed connections">
    {drawable.map((edge, index) => <GraphConnection key={index} edge={edge} from={objects[edge.from]} to={objects[edge.to]}/>)}
  </ul>;
}

export function WorkbenchGraph({ nodes, relations, onConnect }) {
  const [focused, setFocused] = useState('');
  const [source, setSource] = useState('');
  const focusSelector = useRef(null);
  if (!nodes.length) return null;
  const objects = new Map(nodes.map((node) => [node.object_id, { id: node.object_id, kind: node.kind, text: node.text }]));
  const active = objects.get(focused) ?? objects.get(nodes[0].object_id);
  const origin = objects.get(source);
  const connections = relations.filter(({ left_object_id: left, right_object_id: right }) => (
    (left === active.id || right === active.id) && objects.has(left) && objects.has(right)
  ));

  return <article className="review-panel workbench-graph">
    <h3>Investigative graph</h3>
    <label>Focus object <select ref={focusSelector} value={active.id} onChange={(event) => setFocused(event.target.value)}>
      {nodes.map((node) => <option key={node.object_id} value={node.object_id}>{humanize(node.kind)} · {node.text}</option>)}
    </select></label>
    {connections.length ? <ul className="graph-connections" aria-label="Connections to the focused object">
      {connections.map((edge) => <GraphConnection key={edge.relation_id} edge={{ kind: edge.kind, explanation: edge.rationale }}
        from={objects.get(edge.left_object_id)} to={objects.get(edge.right_object_id)} focused={active.id} onOpen={(id) => {
          setFocused(id);
          focusSelector.current?.focus();
        }}/>) }
    </ul> : <p>This object has no recorded links.</p>}
    <div className="row">
      <button type="button" onClick={() => setSource(active.id)}>Start a relation here</button>
      {origin && <>
        <span>Starting object: {origin.text}</span>
        <button type="button" disabled={origin.id === active.id} onClick={() => {
          onConnect(origin.id, active.id);
          setSource('');
        }}>Connect to focused object</button>
        <button type="button" onClick={() => setSource('')}>Cancel link</button>
      </>}
    </div>
  </article>;
}
