# Local run — Epistemic Challenge 004

This procedure is for Windows + Git Bash.

No Qwen inference is required on this machine. DeepSeek is used as the strong-model
surrogate; the experiment is designed to inform a later local/air-gapped deployment.

## 1. Update the experimental branch

```bash
cd /c/dev/prj-niccolo/niccolo

git fetch origin
git switch research/epistemic-challenge-002-20261115
git pull --ff-only

cd experiments/epistemic-challenge-004-2026-11-16
```

## 2. Validate the frozen corpus before any paid call

```bash
python -m unittest -v test_protocol.py test_runner.py
python protocol.py
```

Expected protocol properties include:

- 1,000 cases;
- 250 base situations;
- 4 revisions each;
- 200 holdout cases;
- 250 cases per verdict;
- 200 high-effort cases;
- 30 max-effort cases;
- corpus SHA-256:
  `d296700ec6439a3f1cae406b03ca6b54ca45fcded605f6e4632a0896b9e64c76`.

Do not proceed if the frozen hash differs.

`protocol.py` writes `cases.generated.json` only for local inspection. It is a
reproducible derivative and is not required in Git.

## 3. Configure the API key without writing it to shell history

```bash
read -rsp "DeepSeek API key: " DEEPSEEK_API_KEY; echo
export DEEPSEEK_API_KEY
```

Verify only presence:

```bash
test -n "$DEEPSEEK_API_KEY" && echo "DEEPSEEK_API_KEY configured"
```

## 4. First run: small resumability/cost checkpoint

Start with only 100 new API calls:

```bash
python run_deepseek.py \
  --out results/deepseek-observations.json \
  --budget-usd 20 \
  --max-new-calls 100
```

The runner will stop cleanly and preserve every completed call.

Inspect the printed conservative peak-price estimate. The estimate assumes the
configured peak cache-miss input price and peak output price; actual billing may
be lower due to off-peak pricing or cache hits.

## 5. Continue the same run

If the checkpoint looks normal:

```bash
python run_deepseek.py \
  --out results/deepseek-observations.json \
  --budget-usd 20
```

This resumes from the same JSON. It does not repeat stored calls.

The frozen policy uses:

- Flash non-thinking for the full direct baseline;
- deterministic-first + Flash for semantic abstentions;
- `high` reasoning on 200 structurally selected cases;
- `max` reasoning on 30 of those cases;
- paired direct-high/direct-max observations on the same selected cohorts.

The thinking calls use explicit output caps rather than the API defaults:

- high: 2,048 generated tokens maximum;
- max: 8,192 generated tokens maximum.

If the configured conservative cost reaches USD 20, the runner stops cleanly.
With the current account balance this intentionally leaves a buffer.

## 6. Audit without API calls

After a complete run:

```bash
python audit_results.py \
  --report results/deepseek-observations.json \
  --out results/audit.json
```

The audit reconstructs the corpus, prompts, routing and verdicts without network
access and rejects inconsistencies.

## 7. Commit all experimental artifacts

```bash
git status --short

git add results/deepseek-observations.json results/audit.json
git commit -m "research: record DeepSeek challenge 004 observations"
git push origin research/epistemic-challenge-002-20261115
```

Do not commit `DEEPSEEK_API_KEY`.

If the run stops because of budget or connectivity and you want the partial raw
observations preserved in Git, commit them as a clearly labelled partial
checkpoint rather than overwriting or deleting them.

## 8. Remove the API key from the shell

```bash
unset DEEPSEEK_API_KEY
test -z "$DEEPSEEK_API_KEY" && echo "Key removed from this shell session"
```
