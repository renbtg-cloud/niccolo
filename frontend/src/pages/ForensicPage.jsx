import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { qk } from '../api/keys.js';
import { useAnalysisHistory } from '../hooks/useAnalysisHistory.js';
import { useClient } from '../hooks/useClient.js';
import { useUiStore } from '../stores/ui.js';
import {
  forensicExportFilename,
  forensicItemSafety,
  forensicOriginLabel,
  isForensicItemBound,
  parseForensicPackageText,
  validateForensicFile,
} from '../state/forensicPackage.js';
import { launchApprovedReanalysis } from '../state/reanalysisLaunch.js';

function downloadBlob(blob, filename) {
  const url = URL.createObjectURL(blob);
  try {
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = filename;
    document.body.append(anchor);
    anchor.click();
    anchor.remove();
  } finally {
    window.setTimeout(() => URL.revokeObjectURL(url), 30000);
  }
}

function downloadJson(value, filename) {
  downloadBlob(new Blob([JSON.stringify(value, null, 2)], { type: 'application/json' }), filename);
}

function BatchSafety({ batch }) {
  return <ul className="compact-facts">
    <li><strong>Quarantine:</strong> {batch.import_mode === 'UNTRUSTED' ? 'yes' : batch.import_mode}</li>
    <li><strong>Structure:</strong> {batch.verification_status}</li>
    <li><strong>Structural validity establishes truth:</strong> {batch.structural_validity_establishes_truth ? 'yes' : 'no'}</li>
    <li><strong>Current credentials upgrade old statements:</strong> {batch.current_credentials_upgrade_imported_statements ? 'yes' : 'no'}</li>
    <li><strong>Imported model output is documentary evidence:</strong> {batch.imported_model_output_is_documentary_evidence ? 'yes' : 'no'}</li>
  </ul>;
}

export function ForensicPage() {
  const client = useClient();
  const generation = useUiStore((state) => state.analysis);
  if (!client) return <p>Connect this session first.</p>;
  return <ConnectedForensicPage key={`${client.config.workspace}:${generation}`} client={client}/>;
}

function ConnectedForensicPage({ client }) {
  const qc = useQueryClient();
  const navigate = useNavigate();
  const analysis = useUiStore((state) => state.analysis);
  const setAnalysis = useUiStore((state) => state.setAnalysis);
  const setActiveOperation = useUiStore((state) => state.setActiveOperation);
  const history = useAnalysisHistory();
  const pendingImport = useRef(null);
  const pendingBinding = useRef(null);
  const pendingReanalysis = useRef(null);
  const [file, setFile] = useState(null);
  const [selectedBatch, setSelectedBatch] = useState('');
  const [selectedItem, setSelectedItem] = useState('');
  const [batchCursor, setBatchCursor] = useState(null);
  const [batchBackStack, setBatchBackStack] = useState([]);
  const [itemCursor, setItemCursor] = useState(null);
  const [itemBackStack, setItemBackStack] = useState([]);
  const [note, setNote] = useState('Use as historical context only; do not treat this imported item as fact or evidence.');
  const [reanalysisGoal, setReanalysisGoal] = useState('Reanalyze with the selected historical context');

  const imports = useQuery({
    queryKey: [...qk.forensicImports(client.config), batchCursor],
    queryFn: async () => (await client.forensicImports(batchCursor, 25)).value,
  });
  const batch = useQuery({
    queryKey: ['forensic-import', client.config.workspace, selectedBatch],
    enabled: !!selectedBatch,
    queryFn: async () => (await client.forensicImport(selectedBatch)).value,
  });
  const items = useQuery({
    queryKey: [...qk.forensicImportItems(client.config, selectedBatch), itemCursor],
    enabled: !!selectedBatch,
    queryFn: async () => (await client.forensicImportItems(selectedBatch, itemCursor, 50)).value,
  });
  const detail = useQuery({
    queryKey: ['forensic-import-item', client.config.workspace, selectedBatch, selectedItem],
    enabled: !!selectedBatch && !!selectedItem,
    queryFn: async () => (await client.forensicImportItem(selectedBatch, selectedItem)).value,
  });
  const currentContext = useQuery({
    queryKey: analysis ? qk.analysis(client.config, analysis, 'forensic-context') : ['forensic-context','none'],
    enabled: !!analysis,
    queryFn: async () => (await client.analysisForensicContext(analysis)).value,
  });

  const exportCurrent = useMutation({
    mutationFn: async () => (await client.forensicExport()).value,
    onSuccess: (pkg) => downloadJson(pkg, forensicExportFilename(client.config.situation)),
  });
  const importPackage = useMutation({
    mutationFn: async (selected) => {
      validateForensicFile(selected);
      const parsed = parseForensicPackageText(await selected.text(), selected.name);
      const signature = JSON.stringify({ name: selected.name, size: selected.size, source: parsed.package.manifest?.root_sha256 ?? null });
      if (pendingImport.current?.signature !== signature) {
        pendingImport.current = { signature, batchId: crypto.randomUUID() };
      }
      const batchId = pendingImport.current.batchId;
      const result = await client.importForensic(batchId, parsed.declaredOrigin, parsed.package);
      return { batchId, value: result.value };
    },
    onSuccess: async ({ batchId }) => {
      pendingImport.current = null;
      setFile(null);
      setSelectedBatch(batchId);
      setSelectedItem('');
      setItemCursor(null);
      setItemBackStack([]);
      await qc.invalidateQueries({ queryKey: qk.forensicImports(client.config) });
    },
  });
  const downloadOriginal = useMutation({
    mutationFn: async () => {
      if (!selectedBatch || !selectedItem) throw new Error('Select a retained original item first.');
      return client.downloadForensicOriginal(selectedBatch, selectedItem);
    },
    onSuccess: (blob) => downloadBlob(blob, `niccolo-quarantine-source-${selectedItem}.bin`),
  });
  const bind = useMutation({
    mutationFn: async () => {
      if (!analysis || !selectedBatch || !selectedItem) throw new Error('Select an analysis and imported item first.');
      const cleanNote = note.trim() || null;
      const signature = JSON.stringify({ analysis, selectedBatch, selectedItem, note: cleanNote });
      if (pendingBinding.current?.signature !== signature) {
        pendingBinding.current = { signature, bindingId: crypto.randomUUID() };
      }
      return (await client.bindAnalysisForensicContext(
        analysis,
        pendingBinding.current.bindingId,
        selectedBatch,
        selectedItem,
        cleanNote,
      )).value;
    },
    onSuccess: async () => {
      pendingBinding.current = null;
      await Promise.all([
        qc.invalidateQueries({ queryKey: qk.analysis(client.config, analysis, 'forensic-context') }),
        qc.invalidateQueries({ queryKey: ['analysis'] }),
      ]);
    },
  });

  const reanalyze = useMutation({
    mutationFn: async () => {
      if (!analysis) throw new Error('Select the parent analysis first.');
      const goal = reanalysisGoal.trim() || 'Reanalyze with the selected historical context';
      const signature = JSON.stringify({ analysis, goal });
      if (pendingReanalysis.current?.signature !== signature) {
        pendingReanalysis.current = {
          signature,
          ids: {
            snapshotOperation: crypto.randomUUID(),
            newAnalysis: crypto.randomUUID(),
            agenticOperation: crypto.randomUUID(),
          },
        };
      }
      return launchApprovedReanalysis({
        client,
        priorAnalysis: analysis,
        goal,
        ids: pendingReanalysis.current.ids,
      });
    },
    onSuccess: async (result) => {
      pendingReanalysis.current = null;
      setAnalysis(result.newAnalysis);
      setActiveOperation(result.agenticOperation);
      await Promise.all([
        qc.invalidateQueries({ queryKey: ['analysis'] }),
        qc.invalidateQueries({ queryKey: ['operation'] }),
        qc.invalidateQueries({ queryKey: ['dashboard'] }),
      ]);
      navigate('/operations');
    },
  });

  const importedBatches = imports.data?.batches ?? [];
  const importedItems = items.data?.items ?? [];
  const contextItems = currentContext.data?.items ?? [];
  const alreadyBound = isForensicItemBound(contextItems, selectedBatch, selectedItem);
  const safety = forensicItemSafety(detail.data?.item ?? detail.data);

  const selectBatch = (id) => {
    setSelectedBatch(id);
    setSelectedItem('');
    setItemCursor(null);
    setItemBackStack([]);
  };

  return <section>
    <h2>Prior Niccolò exports</h2>
    <p>Export a Situation for later inspection, or import an older Niccolò package into quarantine. Importing a structurally valid package does <strong>not</strong> make its claims true, turn old model output into evidence, or let current credentials vouch for historical statements.</p>

    <article className="forensic-actions">
      <h3>Export this Situation</h3>
      <p>Creates a deterministic JSON package with provenance and retained source bytes when available. It is intended for traceability and later inspection.</p>
      <button disabled={exportCurrent.isPending} onClick={() => exportCurrent.mutate()}>{exportCurrent.isPending ? 'Preparing export…' : 'Download forensic export'}</button>
      {exportCurrent.error && <p className="error">{exportCurrent.error.message}</p>}
    </article>

    <article className="forensic-actions">
      <h3>Import an older Niccolò export</h3>
      <p>The package is verified structurally, then stored as <strong>untrusted historical material</strong>. Nothing is copied into native evidence, identity, configuration, or analysis tables.</p>
      <input aria-label="Prior Niccolò forensic export" type="file" accept=".json,application/json" disabled={importPackage.isPending} onChange={(event) => {
        pendingImport.current = null;
        importPackage.reset();
        setFile(event.target.files?.[0] ?? null);
      }}/>
      <button disabled={!file || importPackage.isPending} onClick={() => file && importPackage.mutate(file)}>{importPackage.isPending ? 'Importing to quarantine…' : 'Import to quarantine'}</button>
      {file && <small>Selected: {file.name} · {file.size} bytes</small>}
      {importPackage.error && <p className="error" role="alert">{importPackage.error.message}</p>}
      {importPackage.data && <p role="status">Imported to quarantine. Historical statements remain non-evidence until a human explicitly chooses how to use them.</p>}
    </article>

    <article>
      <h3>Quarantined imports</h3>
      {imports.isLoading && <p>Loading imported packages…</p>}
      {imports.error && <p className="error">{imports.error.message}</p>}
      {!imports.isLoading && importedBatches.length === 0 && <p>No prior Niccolò packages have been imported into this workspace.</p>}
      <ul className="forensic-list">{importedBatches.map((entry) => <li key={entry.import_batch_id}>
        <button className={selectedBatch === entry.import_batch_id ? 'history-selected' : ''} onClick={() => selectBatch(entry.import_batch_id)}>{entry.declared_origin}</button>
        <div><small>{entry.item_count} quarantined item(s) · imported {entry.imported_at}</small></div>
      </li>)}</ul>
      <div className="row">
        <button disabled={imports.isFetching || batchBackStack.length === 0} onClick={() => {
          const stack = batchBackStack.slice();
          setBatchCursor(stack.pop() ?? null);
          setBatchBackStack(stack);
        }}>Previous imports</button>
        <button disabled={imports.isFetching || !imports.data?.next_before_batch_id} onClick={() => {
          setBatchBackStack((stack) => [...stack, batchCursor]);
          setBatchCursor(imports.data.next_before_batch_id);
        }}>Next imports</button>
      </div>
    </article>

    {selectedBatch && <article>
      <h3>Selected quarantine batch</h3>
      {batch.isLoading && <p>Loading batch provenance…</p>}
      {batch.error && <p className="error">{batch.error.message}</p>}
      {batch.data && <><p><strong>Declared origin:</strong> {batch.data.declared_origin}</p><p><strong>Original Situation:</strong> <code>{batch.data.source_situation_id}</code></p><BatchSafety batch={batch.data}/></>}
      <h4>Items</h4>
      {items.isLoading && <p>Loading quarantined items…</p>}
      {items.error && <p className="error">{items.error.message}</p>}
      <ul className="forensic-list">{importedItems.map((item) => <li key={item.item_id}>
        <button className={selectedItem === item.item_id ? 'history-selected' : ''} onClick={() => setSelectedItem(item.item_id)}>{forensicOriginLabel(item.origin_class)} · {item.source_path}</button>
        <div><small>{item.source_kind} · {item.source_byte_length} bytes · documentary evidence: {item.documentary_evidence ? 'yes' : 'no'}</small></div>
      </li>)}</ul>
      <div className="row">
        <button disabled={items.isFetching || itemBackStack.length === 0} onClick={() => {
          const stack = itemBackStack.slice();
          setItemCursor(stack.pop() ?? null);
          setItemBackStack(stack);
        }}>Previous items</button>
        <button disabled={items.isFetching || !items.data?.next_after_source_path} onClick={() => {
          setItemBackStack((stack) => [...stack, itemCursor]);
          setItemCursor(items.data.next_after_source_path);
        }}>Next items</button>
      </div>
    </article>}

    {selectedItem && <article>
      <h3>Imported item detail</h3>
      {detail.isLoading && <p>Loading item…</p>}
      {detail.error && <p className="error">{detail.error.message}</p>}
      {detail.data && <>
        <p><strong>{forensicOriginLabel(detail.data.item.origin_class)}</strong> · <code>{detail.data.item.source_path}</code></p>
        <ul className="compact-facts"><li>Quarantine provenance: {safety.quarantine ? 'yes' : 'no'}</li><li>Establishes truth: {safety.establishesTruth ? 'yes' : 'no'}</li><li>Documentary evidence: {safety.documentaryEvidence ? 'yes' : 'no'}</li><li>Original bytes retained: {detail.data.original_bytes_retained ? 'yes' : 'no'}</li></ul>
        {detail.data.payload_json ? <details><summary>Inspect imported historical payload</summary><pre>{JSON.stringify(detail.data.payload_json,null,2)}</pre></details> : <p><small>This item has no JSON preview. Retained original bytes stay quarantined and are never promoted into the current source catalog by downloading them.</small></p>}
        {detail.data.item.source_kind === 'ORIGINAL_SOURCE_BYTES' && detail.data.original_bytes_retained && <div>
          <button disabled={downloadOriginal.isPending} onClick={() => downloadOriginal.mutate()}>{downloadOriginal.isPending ? 'Downloading quarantined bytes…' : 'Download quarantined original bytes'}</button>
          <small> This is a byte-for-byte historical attachment from the imported package, not a native evidence record.</small>
          {downloadOriginal.error && <p className="error" role="alert">{downloadOriginal.error.message}</p>}
        </div>}
      </>}
    </article>}

    <article>
      <h3>Use selected history in current reasoning</h3>
      <p>This is an explicit human choice. Binding an item makes it available as <strong>current non-evidence historical context</strong>; it does not rewrite the frozen analysis input or a previously published answer.</p>
      <label>Analysis<select value={analysis} onChange={(event) => setAnalysis(event.target.value)}><option value="">Select an analysis</option>{(history.data?.analyses ?? []).map((entry) => <option key={entry.analysis_id} value={entry.analysis_id}>{entry.goal || entry.analysis_id}</option>)}</select></label>
      <label>Why include this historical item?<textarea value={note} onChange={(event) => setNote(event.target.value)} maxLength={2048}/></label>
      <button disabled={!analysis || !selectedBatch || !selectedItem || alreadyBound || bind.isPending} onClick={() => bind.mutate()}>{alreadyBound ? 'Already included as historical context' : bind.isPending ? 'Recording choice…' : 'Include as non-evidence context'}</button>
      {bind.error && <p className="error" role="alert">{bind.error.message}</p>}
      {bind.isSuccess && <p role="status">Historical item included as current non-evidence context. The published answer, if any, remains unchanged.</p>}
      {analysis && <details><summary>Current imported context for this analysis</summary>{currentContext.isLoading ? <p>Loading…</p> : <pre>{JSON.stringify(contextItems,null,2)}</pre>}</details>}
      {analysis && contextItems.length > 0 && <div className="forensic-actions">
        <h4>Get a new answer that considers this selected history</h4>
        <p>Reanalysis creates a new immutable analysis linked to the selected parent. The current quarantined selections are carried forward as non-evidence context before reasoning starts; the older published answer remains preserved.</p>
        <label>Reanalysis goal<textarea value={reanalysisGoal} onChange={(event) => setReanalysisGoal(event.target.value)} maxLength={4096}/></label>
        <button disabled={reanalyze.isPending} onClick={() => reanalyze.mutate()}>{reanalyze.isPending ? 'Starting new analysis…' : 'Start new analysis with selected history'}</button>
        {reanalyze.error && <p className="error" role="alert">{reanalyze.error.message}</p>}
      </div>}
    </article>
  </section>;
}
