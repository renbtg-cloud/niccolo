#!/usr/bin/env node
// CI-only OpenAI-compatible protocol fixture. It exercises Niccolo's runtime
// wiring, structured reasoning updates, provenance links, human boundary and
// durable invocation ledger without claiming model quality.
import http from 'node:http';

const port = Number(process.env.NICCOLO_CI_LLM_PORT ?? '18080');
const host = '127.0.0.1';
const ids = {
  assumption: '11111111-1111-4111-8111-111111111111',
  interpretation: '22222222-2222-4222-8222-222222222222',
  evidenceLink: '33333333-3333-4333-8333-333333333333',
  claim: '44444444-4444-4444-8444-444444444444',
  artifactAssumption: '55555555-5555-4555-8555-555555555555',
  alternative: '66666666-6666-4666-8666-666666666666',
  gap: '77777777-7777-4777-8777-777777777777',
  clarification: '88888888-8888-4888-8888-888888888888',
  challenge: '99999999-9999-4999-8999-999999999999',
};

function packet(request) {
  return request?.input?.reasoning_packet ?? {};
}
function firstEvidenceId(request) {
  return packet(request)?.frozen_input?.source_fragments?.[0]?.evidence_id ?? null;
}
function responseFor(request) {
  const purpose = request.purpose;
  const evidenceId = firstEvidenceId(request);
  if (purpose === 'CHECK_ASSUMPTIONS') {
    return {
      purpose,
      status: 'deterministic-ci-protocol-smoke',
      niccolo_update: {
        assumptions: [{
          assumption_id: ids.assumption,
          statement: 'Imported timestamps and speaker labels are source content, not independently verified facts.',
          basis_note: 'The frozen input contains imported messaging records.',
        }],
        evidence_gaps: [],
        clarifications: [],
      },
    };
  }
  if (purpose === 'CHECK_ALTERNATIVES') {
    return {
      purpose,
      status: 'deterministic-ci-protocol-smoke',
      niccolo_update: {
        candidate_interpretations: [{
          interpretation_id: ids.interpretation,
          text: 'The source material may admit an ordinary explanation that is not captured by a single narrative.',
          rationale: 'The records contain attributed statements but do not establish an unstated motive.',
          predictions: [],
          evidence_links: evidenceId ? [{
            link_id: ids.evidenceLink,
            evidence_id: evidenceId,
            relation: 'CONTEXT',
            rationale: 'The imported fragment supplies context without establishing motive or identity.',
          }] : [],
          relations: [],
        }],
        assumptions: [],
        evidence_gaps: [],
        clarifications: [],
      },
    };
  }
  if (purpose === 'SYNTHESIZE') {
    const p = packet(request);
    if (!(p?.lens?.assumptions ?? []).some(item => item.assumption_id === ids.assumption)) {
      throw new Error('prior assumption missing from synthesis packet');
    }
    if (!(p?.candidate_interpretations ?? []).some(item => item.interpretation_id === ids.interpretation)) {
      throw new Error('prior interpretation missing from synthesis packet');
    }
    if (!evidenceId) throw new Error('source evidence missing from synthesis packet');
    return {
      purpose,
      status: 'deterministic-ci-protocol-smoke',
      niccolo_artifact: {
        answer_text: 'The imported records contain attributed statements, but this fixture does not infer motive or truth beyond those records.',
        coverage_note: 'Protocol validation used the frozen imported records; unverified context remains explicitly unresolved.',
        claims: [{
          claim_id: ids.claim,
          ordinal: 1,
          kind: 'OBSERVATION',
          text: 'The imported record contains a message attributed to a speaker label.',
          qualification: 'A source speaker label is attribution in the record, not verified identity.',
        }],
        source_links: [{
          claim_id: ids.claim,
          evidence_id: evidenceId,
          relation: 'SUPPORTS_AS_USED',
        }],
        assumptions: [{
          assumption_id: ids.artifactAssumption,
          claim_id: ids.claim,
          statement: 'The imported timestamp is treated only as part of the record presentation.',
          basis_note: 'No independent timestamp verification is present in this CI fixture.',
        }],
        alternatives: [{
          alternative_id: ids.alternative,
          claim_id: ids.claim,
          text: 'An ordinary explanation may fit the same limited record.',
          rationale: 'The imported statements do not establish an unstated motive.',
        }],
        evidence_gaps: [{
          gap_id: ids.gap,
          claim_id: ids.claim,
          description: 'The examined records do not independently establish why the reported events occurred.',
          requested_source_kind: 'contemporaneous explanation',
        }],
        clarifications: [{
          question_id: ids.clarification,
          text: 'Is there contemporaneous evidence explaining the event beyond the imported messages?',
          reason: 'That could discriminate ordinary explanations from motive-based interpretations.',
        }],
      },
    };
  }
  if (purpose === 'CHALLENGE_SYNTHESIS') {
    return {
      purpose,
      status: 'deterministic-ci-protocol-smoke',
      niccolo_challenge: {
        challenges: [{
          challenge_id: ids.challenge,
          kind: 'COVERAGE_GAP',
          text: 'The synthesis is based on a small imported record set and cannot establish unstated motive.',
          materiality: 80,
        }],
      },
    };
  }
  if (['ASSESS_CONTEXT', 'PROPOSE_CLARIFICATION'].includes(purpose)) {
    return {
      purpose,
      status: 'deterministic-ci-protocol-smoke',
      niccolo_update: { assumptions: [], evidence_gaps: [], clarifications: [] },
    };
  }
  return { purpose, status: 'deterministic-ci-protocol-smoke' };
}

const server = http.createServer((req, res) => {
  if (req.method === 'GET' && req.url === '/healthz') {
    res.writeHead(200, { 'content-type': 'application/json' });
    res.end('{"status":"ok"}');
    return;
  }
  if (req.method !== 'POST' || req.url !== '/v1/chat/completions') {
    res.writeHead(404);
    res.end();
    return;
  }
  let raw = '';
  req.setEncoding('utf8');
  req.on('data', chunk => { raw += chunk; });
  req.on('end', () => {
    try {
      const body = JSON.parse(raw || '{}');
      const user = [...(body.messages ?? [])].reverse().find(message => message.role === 'user')?.content ?? '{}';
      const modelRequest = JSON.parse(user);
      const content = JSON.stringify(responseFor(modelRequest));
      const payload = JSON.stringify({
        id: 'chatcmpl-niccolo-ci-smoke',
        object: 'chat.completion',
        model: body.model ?? 'niccolo-ci-protocol-smoke',
        choices: [{ index: 0, message: { role: 'assistant', content }, finish_reason: 'stop' }],
        usage: { prompt_tokens: 23, completion_tokens: 19, total_tokens: 42 },
      });
      res.writeHead(200, { 'content-type': 'application/json', 'content-length': Buffer.byteLength(payload) });
      res.end(payload);
    } catch (error) {
      const payload = JSON.stringify({ error: String(error) });
      res.writeHead(400, { 'content-type': 'application/json', 'content-length': Buffer.byteLength(payload) });
      res.end(payload);
    }
  });
});

server.listen(port, host, () => {
  process.stdout.write(`CI OpenAI-compatible mock listening on http://${host}:${port}/v1\n`);
});
for (const signal of ['SIGTERM', 'SIGINT']) {
  process.on(signal, () => server.close(() => process.exit(0)));
}
