const bytes = (value) => new TextEncoder().encode(value).length;
const oneLine = (value, max) => {
  const cleaned = String(value ?? '').trim();
  if (!cleaned || bytes(cleaned) > max || /\p{Cc}/u.test(cleaned)) throw new Error('Enter bounded single-line text.');
  return cleaned;
};
const optionalOneLine = (value, max) => String(value ?? '').trim() ? oneLine(value, max) : null;
const optionalText = (value, max) => {
  const cleaned = String(value ?? '').trim();
  if (!cleaned) return null;
  if (bytes(cleaned) > max || cleaned.includes('\0')) throw new Error('Text exceeds the allowed size.');
  return cleaned;
};

export function artifactRequest(draft) {
  const length = String(draft.declaredByteLength ?? '').trim();
  if (length && !/^\d+$/u.test(length)) throw new Error('Declared byte length must be a non-negative integer.');
  const number = length ? Number(length) : null;
  if (number !== null && !Number.isSafeInteger(number)) throw new Error('Declared byte length is too large for this client.');
  const sha = String(draft.declaredSha256 ?? '').trim();
  if (sha && !/^[a-f0-9]{64}$/u.test(sha)) throw new Error('Declared SHA-256 must contain 64 lowercase hexadecimal characters.');
  return {
    artifact_type: oneLine(draft.artifactType, 128),
    label: oneLine(draft.label, 1024),
    declared_mime_type: optionalOneLine(draft.declaredMimeType, 255),
    declared_byte_length: number,
    declared_sha256: sha || null,
  };
}

export function storageReferenceRequest(draft, accessEventId) {
  const locator = oneLine(draft.locator, 4096);
  if (/[?&](?:x-amz-signature|x-amz-credential|x-goog-signature|signature|sig|token|access_token|credential)=/iu.test(locator)) {
    throw new Error('Do not store credentials or signed access tokens in an artifact locator.');
  }
  const allowed = new Set(['UNKNOWN','NONE','AVAILABLE','TEMPORARILY_UNAVAILABLE','ACCESS_DENIED','NOT_FOUND','UNVERIFIED']);
  if (!allowed.has(draft.accessStatus)) throw new Error('Choose a valid declared access state.');
  return {
    access_event_id: accessEventId,
    storage_type: oneLine(draft.storageType, 64),
    locator,
    provider: optionalOneLine(draft.provider, 256),
    environment_label: optionalOneLine(draft.environmentLabel, 256),
    resolver_profile: optionalOneLine(draft.resolverProfile, 256),
    notes: optionalText(draft.notes, 4096),
    access_status: draft.accessStatus,
    access_note: optionalText(draft.accessNote, 4096),
  };
}

export function representationRequest(draft) {
  const producers = new Set(['USER','THIRD_PARTY_HUMAN','EXTERNAL_SYSTEM','UNKNOWN']);
  const completeness = new Set(['COMPLETE','PARTIAL','UNKNOWN']);
  if (!producers.has(draft.producerType)) throw new Error('Choose who produced this representation.');
  if (!completeness.has(draft.completenessStatus)) throw new Error('Choose the representation completeness.');
  const content = optionalText(draft.content, 65536);
  if (!content) throw new Error('Enter the explicitly supplied representation text.');
  return {
    representation_type: oneLine(draft.representationType, 128),
    producer_type: draft.producerType,
    producer_label: optionalOneLine(draft.producerLabel, 512),
    production_method: optionalOneLine(draft.productionMethod, 1024),
    scope_locator: oneLine(draft.scopeLocator, 2048),
    content,
    completeness_status: draft.completenessStatus,
    epistemic_qualification: optionalText(draft.epistemicQualification, 4096),
    limitations: optionalText(draft.limitations, 4096),
  };
}
