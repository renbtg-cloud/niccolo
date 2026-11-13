import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const app = readFileSync(new URL('../App.jsx', import.meta.url), 'utf8');
const connect = readFileSync(new URL('./ConnectPanel.jsx', import.meta.url), 'utf8');
const situation = readFileSync(new URL('./SituationPage.jsx', import.meta.url), 'utf8');
const review = readFileSync(new URL('./ReviewPage.jsx', import.meta.url), 'utf8');
const feedback = readFileSync(new URL('./FeedbackPage.jsx', import.meta.url), 'utf8');
const questionsHook = readFileSync(new URL('../hooks/useQuestions.js', import.meta.url), 'utf8');
const analysis = readFileSync(new URL('./AnalysisPage.jsx', import.meta.url), 'utf8');
const identities = readFileSync(new URL('./IdentitiesPage.jsx', import.meta.url), 'utf8');

describe('buyer-facing primary surfaces', () => {
  it('remounts every route after changing the authenticated session', () => {
    expect(app).toContain('const generation=useSessionStore(s=>s.generation)');
    expect(app).toContain('<Routes key={generation}>');
  });

  it('collapses prepared demo identifiers while keeping the bearer token explicit', () => {
    expect(connect).toContain('Prepared Situation');
    expect(connect).toContain('<summary>Connection identifiers</summary>');
    expect(connect).toContain('Bearer tokens are never placed in the URL or persisted.');
    expect(connect).toContain('A local one-shot handoff connects automatically when present');
    expect(connect).toContain('.niccolo-local/token.txt');
    expect(connect).toContain('Share prepared demo link');
    expect(connect).toContain('Send the bearer token separately.');
  });

  it('describes Situation preparation without blocking a first pass', () => {
    expect(situation).toContain('What Niccolò has to work with');
    expect(situation).toContain('not a completeness score and not a gate');
    expect(situation).toContain('Analyze this Situation →');
    expect(situation).toContain('Connect this session first.');
    expect(situation).not.toContain('Connect to the BFF first.');
  });

  it('keeps the Analyze screen grounded in the selected Situation before launching work', () => {
    expect(analysis).not.toContain('This prepared Situation has not been created yet.');
    const operations = readFileSync(new URL('./OperationsPage.jsx', import.meta.url), 'utf8');
    expect(operations).toContain("queryKey:client?qk.situation(client.config):['situation','none']");
    expect(operations).toContain('situationGuide(situation.data)');
    expect(operations).toContain('Focus-only first pass.');
    expect(operations).toContain('bash scripts/configure-local-model.sh');
  });

  it('shows model readiness before the user enters Analyze', () => {
    expect(situation).toContain("queryKey: ['mvp-runtime-readiness']");
    expect(situation).toContain('A reasoning model is not configured yet.');
    expect(situation).toContain('bash scripts/configure-local-model.sh');
    expect(situation).toContain('.niccolo-local/model.env');
  });

  it('remounts Situation-local drafts and cursors when the connection changes', () => {
    expect(situation).toContain('function ConnectedSituation({client})');
    expect(situation).toContain('const generation = useSessionStore((state) => state.generation)');
    expect(situation).toContain('key={generation}');
    expect(situation).toContain('if (!client) return <p>Connect this session first.</p>');
  });

  it('separates persisted focus from draft state across Situation changes', () => {
    expect(situation).toContain('key={JSON.stringify([client.config.workspace, client.config.situation, snapshot.focus])}');
    expect(situation).toContain('initial={snapshot.focus}');
    expect(situation).not.toContain('snapshot.revision])}');
  });

  it('retains each selected file identity when a partial import must be retried', () => {
    expect(situation).toContain('pendingDocumentIds.current.get(selected)');
    expect(situation).toContain('pendingDocumentIds.current.set(selected, documentId)');
    expect(situation).toContain('pendingDocumentIds.current.clear()');
    expect(situation).toContain('onError: refresh');
    expect(situation).toContain('upload.reset();');
    expect(situation).toContain('pendingContribution.current?.signature !== signature');
    expect(situation).toContain('pendingCorrection.current?.signature !== signature');
    expect(situation).toContain('pendingContribution.current.id');
    expect(situation).toContain('pendingCorrection.current.id');
  });

  it('makes append-only identity mapping history navigable beyond its first page', () => {
    expect(identities).toContain('[...qk.identityHistory(client.config, documentId), historyCursor]');
    expect(identities).toContain('client.identityHistory(documentId, historyCursor, 100)');
    expect(identities).toContain('Previous mapping history');
    expect(identities).toContain('Next mapping history');
    expect(identities).toContain('history.data?.next_after');
  });

  it('does not keep a hidden unassigned person choice after the people page changes', () => {
    expect(identities).toContain("choice !== '__new__'");
    expect(identities).toContain("choice !== binding?.person_id");
    expect(identities).toContain("!identities.some((item) => item.person_id === choice)");
  });

  it('keeps the current mapping selectable when its person is outside the current identity page', () => {
    expect(identities).toContain('binding?.person_id && !current');
    expect(identities).toContain('mapped person outside this page');
  });

  it('clears stale identity-mapping drafts after the persisted binding advances', () => {
    expect(identities).toContain('[binding?.event_id, binding?.person_id]');
    expect(identities).toContain("setNewLabel('')");
    expect(identities).toContain("setReason('')");
  });

  it('reuses a newly created person when only the mapping reason changes after a partial retry', () => {
    expect(identities).toContain('newPersonSignature');
    expect(identities).toContain('pendingAssign.current?.newPersonSignature === newPersonSignature');
    expect(identities).toContain('reusablePersonId || crypto.randomUUID()');
  });

  it('keeps the Analysis UUID form in sync with history selection', () => {
    expect(analysis).toContain('useEffect(()=>{reset({analysis});},[analysis,reset]);');
    expect(analysis).toContain('defaultValues:{analysis}');
  });

  it('refreshes a published analysis output exactly when an observed live operation succeeds', () => {
    const observer = readFileSync(new URL('../hooks/useOperationObserver.js', import.meta.url), 'utf8');
    expect(observer).toContain('completedAnalysisOutputRefreshId(previous, observed)');
    expect(observer).toContain("qk.analysis(client.config, completedAnalysis, 'output')");
  });

  it('keeps selected analyses observable outside the Analyze screen', () => {
    for (const surface of [review, analysis]) {
      expect(surface).toContain('useOperationObserver(activeOperation)');
      expect(surface).toContain('observed.operation?.target_analysis_id===analysis');
      expect(surface).toContain('useAnalysisOutput(analysis,');
    }
  });

  it('makes sources and human identity catalogs navigable past the first page', () => {
    expect(situation).toContain('Next sources');
    expect(situation).toContain('Previous sources');
    expect(situation).toContain('client.sources(sourceCursor, 100)');
    expect(identities).toContain('Next source records');
    expect(identities).toContain('Next people');
    expect(identities).toContain('client.identities(identityCursor, 100)');
    expect(identities).toContain('resetDocumentSelection');
  });

  it('retries speaker identity creation and binding with stable identifiers', () => {
    expect(identities).toContain('pendingAssign.current?.signature !== signature');
    expect(identities).toContain('client.createIdentity(attempt.personId, newLabel)');
    expect(identities).toContain('client.recordIdentityEvent(attempt.eventId');
    expect(identities).toContain('pendingClear.current.eventId');
    expect(identities).toContain('pendingAssign.current = null');
    expect(identities).toContain('pendingClear.current = null');
  });

  it('pages both current positions and immutable feedback events', () => {
    expect(feedback).toContain('questionFeedback(question.question_id,eventCursor,100)');
    expect(feedback).toContain('questionPositions(question.question_id,positionsCursor,100)');
    expect(feedback).toContain('Previous feedback events');
    expect(feedback).toContain('Next feedback events');
    expect(feedback).toContain('Previous positions');
    expect(feedback).toContain('Next positions');
    expect(feedback).toContain('history.data.next_after');
    expect(feedback).toContain('positions.data.next_after');
  });

  it('navigates human feedback beyond the first 100 questions', () => {
    expect(feedback).toContain('useQuestions(questionCursor)');
    expect(feedback).toContain('Previous questions');
    expect(feedback).toContain('Next questions');
    expect(feedback).toContain('questions.data.next_after');
    expect(questionsHook).toContain("['questions', client.config.workspace, after]");
    expect(questionsHook).toContain('client.questions(after, 100)');
  });

  it('does not mutate in-flight feedback payloads and confirms stored results', () => {
    expect(feedback).toContain('disabled={submit.isPending}');
    expect(feedback).toContain('disabled={create.isPending}');
    expect(feedback).toContain('submit.reset()');
    expect(feedback).toContain('create.reset()');
    expect(feedback).toContain('Your position was recorded.');
    expect(feedback).toContain('Investigative question recorded.');
  });

  it('retries user questions and feedback with stable record identities', () => {
    expect(feedback).toContain('pendingFeedback.current?.signature !== signature');
    expect(feedback).toContain('pendingFeedback.current.eventId');
    expect(feedback).toContain('pendingFeedback.current=null');
    expect(feedback).toContain('pendingQuestion.current?.question !== question');
    expect(feedback).toContain('pendingQuestion.current.questionId');
    expect(feedback).toContain('pendingQuestion.current=null');
  });

  it('remounts review drafts when selecting another analysis', () => {
    expect(review).toContain("const selectedAnalysis=useUiStore(s=>s.analysis)");
    expect(review).toContain("<ReviewContent key={selectedAnalysis||'unselected'} />");
    expect(review).toContain('function ReviewContent()');
  });

  it('shows a compact human briefing and transparent bounded lead prioritization', () => {
    expect(review).toContain('<h3>At a glance</h3>');
    expect(review).toContain('This briefing is descriptive only');
    expect(review).toContain('does not rank for you, decide for you, start another analysis or execute an action');
    expect(review).toContain('Showing ${leadMeta.shownCount} prioritized leads from ${leadMeta.candidateCount} recorded candidates.');
    expect(review).toContain('deterministic information-value heuristic');
    expect(review).toContain('not confidence, probability or truth');
    expect(review).toContain('You choose whether any lead is worth pursuing.');
  });

  it('keeps Review self-contained with answer and concrete competing interpretations', () => {
    expect(review).toContain("<h3>Niccolò's answer</h3>");
    expect(review).toContain('Competing interpretations considered');
    expect(review).toContain('These remain candidates, not facts, and are not automatically ranked.');
  });
});
