#!/usr/bin/env node
// CI-only OpenAI-compatible protocol fixture. It proves Niccolo's runtime wiring,
// durable invocation ledger and artifact publication without claiming reasoning quality.
import http from 'node:http';

const port = Number(process.env.NICCOLO_CI_LLM_PORT ?? '18080');
const host = '127.0.0.1';

function responseFor(purpose) {
  if (purpose === 'SYNTHESIZE') {
    return {
      purpose,
      status: 'deterministic-ci-protocol-smoke',
      niccolo_artifact: {
        answer_text: 'Deterministic CI protocol smoke synthesis. No truth claim.',
        coverage_note: 'Protocol/wiring validation only; not a quality evaluation.',
        claims: [],
      },
    };
  }
  if (['ASSESS_CONTEXT', 'CHECK_ASSUMPTIONS', 'PROPOSE_CLARIFICATION', 'CHECK_ALTERNATIVES'].includes(purpose)) {
    return {
      purpose,
      status: 'deterministic-ci-protocol-smoke',
      niccolo_update: { assumptions: [], evidence_gaps: [], clarifications: [] },
    };
  }
  if (purpose === 'CHALLENGE_SYNTHESIS') {
    return {
      purpose,
      status: 'deterministic-ci-protocol-smoke',
      niccolo_challenge: { challenges: [] },
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
      const content = JSON.stringify(responseFor(modelRequest.purpose ?? 'UNKNOWN'));
      const payload = JSON.stringify({
        id: 'chatcmpl-niccolo-ci-smoke',
        object: 'chat.completion',
        model: body.model ?? 'niccolo-ci-protocol-smoke',
        choices: [{ index: 0, message: { role: 'assistant', content }, finish_reason: 'stop' }],
        usage: { prompt_tokens: 11, completion_tokens: 7, total_tokens: 18 },
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
