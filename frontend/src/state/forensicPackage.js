export const FORENSIC_FORMAT = 'niccolo.situation-forensic-export';
export const FORENSIC_FORMAT_VERSION = 1;
export const MAX_BROWSER_FORENSIC_IMPORT_BYTES = 180 * 1024 * 1024;

const originLabels = {
  IMPORTED_HUMAN_ASSERTION: 'Imported human assertion',
  IMPORTED_MACHINE_OUTPUT: 'Imported model / machine output',
  IMPORTED_CONFIGURATION_ASSERTION: 'Imported configuration assertion',
  IMPORTED_SOURCE_ARTIFACT: 'Imported source artifact',
  IMPORTED_IDENTITY_ASSERTION: 'Imported identity assertion',
  IMPORTED_HISTORICAL_STATE: 'Imported historical state',
};

export function parseForensicPackageText(text, filename = 'niccolo-export.json') {
  if (typeof text !== 'string' || !text.trim()) throw new Error('The selected file is empty.');
  let pkg;
  try {
    pkg = JSON.parse(text);
  } catch {
    throw new Error('The selected file is not valid JSON.');
  }
  if (!pkg || typeof pkg !== 'object' || Array.isArray(pkg)) throw new Error('The selected JSON is not a Niccolò package.');
  if (pkg.format !== FORENSIC_FORMAT || pkg.format_version !== FORENSIC_FORMAT_VERSION) {
    throw new Error('The selected JSON is not a supported Niccolò forensic export.');
  }
  if (typeof pkg.workspace_id !== 'string' || typeof pkg.situation_id !== 'string' || !pkg.manifest) {
    throw new Error('The Niccolò package is missing required provenance metadata.');
  }
  const safeName = String(filename || 'niccolo-export.json').replace(/[\r\n\0]/g, '').slice(0, 512);
  return { package: pkg, declaredOrigin: `local-file:${safeName}` };
}

export function validateForensicFile(file) {
  if (!file) throw new Error('Choose a Niccolò JSON export first.');
  if (file.size > MAX_BROWSER_FORENSIC_IMPORT_BYTES) {
    throw new Error('This export is too large for the browser import surface.');
  }
  const name = String(file.name || '').toLowerCase();
  if (name && !name.endsWith('.json')) throw new Error('Choose a .json Niccolò forensic export.');
  return file;
}

export function forensicExportFilename(situationId) {
  const id = String(situationId || 'situation').replace(/[^a-zA-Z0-9_-]/g, '').slice(0, 64) || 'situation';
  return `niccolo-situation-${id}-forensic.json`;
}

export function forensicOriginLabel(originClass) {
  return originLabels[originClass] ?? 'Imported historical material';
}

export function forensicItemSafety(item) {
  return {
    quarantine: item?.quarantine_provenance !== false,
    establishesTruth: item?.establishes_truth === true || item?.current_credentials_establish_truth === true,
    documentaryEvidence: item?.documentary_evidence === true,
  };
}

export function isForensicItemBound(contextItems, batchId, itemId) {
  return (contextItems ?? []).some((item) => item.import_batch_id === batchId && item.item_id === itemId);
}
