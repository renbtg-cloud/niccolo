import { describe, expect, it } from 'vitest';
import { artifactRequest, representationRequest, storageReferenceRequest } from './artifacts.js';

describe('parser-free artifact state', () => {
  it('records declared identity without accepting file bytes', () => {
    expect(artifactRequest({artifactType:' VIDEO ',label:' meeting.mp4 ',declaredByteLength:'18000000000'})).toEqual({
      artifact_type:'VIDEO',label:'meeting.mp4',declared_mime_type:null,
      declared_byte_length:18000000000,declared_sha256:null,
    });
  });
  it('rejects credentials in durable locators', () => {
    expect(() => storageReferenceRequest({storageType:'HTTPS',locator:'https://x.invalid/a?X-Amz-Signature=secret',accessStatus:'UNVERIFIED'},'event')).toThrow(/credentials/);
  });
  it('preserves multiline human text without claiming it was parsed', () => {
    expect(representationRequest({
      representationType:'HUMAN_TRANSCRIPTION',producerType:'USER',scopeLocator:'pages 7-9',
      content:'First line\nSecond line',completenessStatus:'PARTIAL',
    })).toMatchObject({
      representation_type:'HUMAN_TRANSCRIPTION',producer_type:'USER',scope_locator:'pages 7-9',
      content:'First line\nSecond line',completeness_status:'PARTIAL',
    });
  });
});
