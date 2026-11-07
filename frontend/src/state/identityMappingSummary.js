export function identityMappingSummary(speakers, bindings) {
  const rows = speakers ?? [];
  const mappedLabels = new Set((bindings ?? []).filter((item) => item.person_id).map((item) => item.speaker_label));
  const mappedCount = rows.filter((speaker) => mappedLabels.has(speaker.speaker_label)).length;
  return {
    speakerCount: rows.length,
    mappedCount,
    unmappedCount: rows.length - mappedCount,
    allMapped: rows.length > 0 && mappedCount === rows.length,
    mappingEstablishesIdentityTruth: false,
  };
}
