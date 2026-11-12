import { describe, expect, it } from 'vitest';
import {
  FORENSIC_FORMAT,
  FORENSIC_FORMAT_VERSION,
  forensicExportFilename,
  forensicItemSafety,
  forensicOriginLabel,
  isForensicItemBound,
  parseForensicPackageText,
  validateForensicFile,
} from './forensicPackage.js';

const pkg = {
  format: FORENSIC_FORMAT,
  format_version: FORENSIC_FORMAT_VERSION,
  workspace_id: '11111111-1111-4111-8111-111111111111',
  situation_id: '22222222-2222-4222-8222-222222222222',
  manifest: { algorithm: 'sha256', entries: [], root_sha256: 'a'.repeat(64) },
};

describe('forensic package browser boundary', () => {
  it('accepts only the versioned Niccolò forensic envelope and records local filename provenance', () => {
    const parsed = parseForensicPackageText(JSON.stringify(pkg), 'prior case.json');
    expect(parsed.package).toEqual(pkg);
    expect(parsed.declaredOrigin).toBe('local-file:prior case.json');
    expect(() => parseForensicPackageText('{}')).toThrow(/supported Niccolò forensic export/);
    expect(() => parseForensicPackageText('{broken')).toThrow(/valid JSON/);
  });

  it('rejects oversized and misleading browser files before parsing', () => {
    expect(() => validateForensicFile({ name: 'case.txt', size: 100 })).toThrow(/\.json/);
    expect(() => validateForensicFile({ name: 'case.json', size: 181 * 1024 * 1024 })).toThrow(/too large/);
    expect(validateForensicFile({ name: 'case.json', size: 100 }).name).toBe('case.json');
  });

  it('keeps imported material explicitly non-truth and non-evidence by default', () => {
    expect(forensicOriginLabel('IMPORTED_MACHINE_OUTPUT')).toMatch(/model/);
    expect(forensicItemSafety({ current_credentials_establish_truth: false, documentary_evidence: false })).toEqual({
      quarantine: true,
      establishesTruth: false,
      documentaryEvidence: false,
    });
  });

  it('detects an existing explicit binding by provenance identity', () => {
    const context = [{ import_batch_id: 'batch', item_id: 'item' }];
    expect(isForensicItemBound(context, 'batch', 'item')).toBe(true);
    expect(isForensicItemBound(context, 'batch', 'other')).toBe(false);
  });

  it('uses a filesystem-safe bounded export filename', () => {
    expect(forensicExportFilename('abc/../../def')).toBe('niccolo-situation-abcdef-forensic.json');
  });
});
