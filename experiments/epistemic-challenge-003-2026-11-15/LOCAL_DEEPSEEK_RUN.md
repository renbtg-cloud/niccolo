# Local DeepSeek run — Epistemic Challenge 003

This guide runs the DeepSeek extension of Challenge 003 on a Windows machine using Git Bash.

## Important: do not run Qwen locally

This run uses only the DeepSeek API. Do not download, reconstruct or load the Qwen GGUF on the local machine.

The existing Qwen result is already versioned in:

```text
experiments/epistemic-challenge-003-2026-11-15/results/observations.json
```

The new DeepSeek result will be written to:

```text
experiments/epistemic-challenge-003-2026-11-15/results/deepseek-observations.json
```

Comparison therefore uses the already-recorded Qwen run versus the new DeepSeek API run.

Rust and VS Code Rust extensions may already be installed, but they are not required for this experiment. The DeepSeek runner uses only the Python standard library.

## 1. Enter the repository and protect local work

In Git Bash:

```bash
cd /c/path/to/niccolo
git status --short
```

If there are important local changes, commit or otherwise preserve them before switching branches.

## 2. Checkout the experimental branch

The branch is:

```text
research/epistemic-challenge-002-20261115
```

Run:

```bash
git fetch origin

git switch research/epistemic-challenge-002-20261115 2>/dev/null \
  || git switch --track -c research/epistemic-challenge-002-20261115 \
       origin/research/epistemic-challenge-002-20261115

git pull --ff-only

git branch --show-current
git rev-parse HEAD
```

The first verification command must print:

```text
research/epistemic-challenge-002-20261115
```

Do not hard-reset to a SHA from this document. The remote branch is authoritative because this experimental branch may receive additional commits.

## 3. Enter Challenge 003

```bash
cd experiments/epistemic-challenge-003-2026-11-15
ls
```

Expected files include:

```text
README.md
cases.json
protocol.py
run.py
run_deepseek_challenge003.py
test_protocol.py
test_results.py
results/
```

`run.py` is the existing Qwen runner.

`run_deepseek_challenge003.py` is the DeepSeek API runner.

Do not use `run.py` on this local machine for this comparison.

## 4. Check Python

```bash
python --version
```

If `python` is not available, try:

```bash
py -3 --version
```

If `py -3` is the working command, substitute it for `python` below.

No `pip install` is required.

## 5. Configure the DeepSeek API key without writing it to shell history

```bash
read -rsp "DeepSeek API key: " DEEPSEEK_API_KEY; echo
export DEEPSEEK_API_KEY
```

Verify only that the variable exists:

```bash
test -n "$DEEPSEEK_API_KEY" && echo "DEEPSEEK_API_KEY configured"
```

Do not put the key in repository files, results, commits, Markdown, screenshots or copied console output.

## 6. Smoke-test the API

The first run uses:

```text
model: deepseek-flash
thinking: disabled
```

As of November 2026, `deepseek-flash` is the official API name for the latest DeepSeek V4.1 Flash model, and Chat Completions supports `thinking.type = disabled`.

Smoke test:

```bash
curl -fsS https://api.deepseek.com/chat/completions \
  -H "Authorization: Bearer $DEEPSEEK_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "deepseek-flash",
    "thinking": {"type": "disabled"},
    "messages": [
      {"role": "user", "content": "Reply with exactly: OK"}
    ],
    "max_tokens": 8,
    "temperature": 0
  }'
```

Do not proceed if the API returns HTTP 401.

## 7. Run the deterministic tests first

```bash
python -m unittest -v test_protocol.py test_results.py
```

These tests do not spend DeepSeek tokens.

## 8. Run DeepSeek

Recommended first run:

```bash
python run_deepseek_challenge003.py \
  --model deepseek-flash \
  --thinking disabled \
  --out results/deepseek-observations.json
```

The runner keeps the frozen Challenge 003 cases and deterministic verifier unchanged.

It measures:

- DeepSeek direct on the complete case;
- deterministic lexical parsing;
- deterministic-first Niccolò routing with DeepSeek only where the lexical parser abstains;
- the conservative stable-only policy.

The current frozen design requires up to roughly 59 API calls: 40 direct case calls plus the semantic classifications routed by deterministic abstention.

The runner saves each completed call atomically. If connectivity fails, rerun the same command; completed calls are reused.

## 9. Keep the first comparison cheap and controlled

For the first run, keep:

```text
deepseek-flash
thinking disabled
temperature 0
```

Do not mix a V4 Pro or thinking-enabled run into the first comparison.

Those can be separate experimental conditions later.

## 10. Compare with the already-versioned Qwen result

After DeepSeek completes:

```bash
ls -lh results/observations.json
ls -lh results/deepseek-observations.json
```

The files are:

```text
Qwen:
results/observations.json

DeepSeek:
results/deepseek-observations.json
```

No local Qwen inference is necessary.

## 11. Commit the experimental result

Experimental artifacts belong in the experimental branch.

First inspect:

```bash
git status --short
git diff -- results/deepseek-observations.json
```

Then commit the result:

```bash
git add results/deepseek-observations.json

git commit -m "research: record DeepSeek challenge 003 observations"

git push origin research/epistemic-challenge-002-20261115
```

Do not commit the API key. The runner never writes `DEEPSEEK_API_KEY` into the result.

If additional analysis files are generated from this run, commit those too with a research-oriented commit message. Preserve raw observations; do not overwrite the Qwen result.

## 12. Remove the key from the Git Bash session

```bash
unset DEEPSEEK_API_KEY

test -z "$DEEPSEEK_API_KEY" && echo "Key removed from this shell session"
```

## Minimal command sequence

```bash
cd /c/path/to/niccolo

git fetch origin
git switch research/epistemic-challenge-002-20261115 2>/dev/null \
  || git switch --track -c research/epistemic-challenge-002-20261115 \
       origin/research/epistemic-challenge-002-20261115
git pull --ff-only

cd experiments/epistemic-challenge-003-2026-11-15

read -rsp "DeepSeek API key: " DEEPSEEK_API_KEY; echo
export DEEPSEEK_API_KEY

python -m unittest -v test_protocol.py test_results.py

python run_deepseek_challenge003.py \
  --model deepseek-flash \
  --thinking disabled \
  --out results/deepseek-observations.json

git add results/deepseek-observations.json
git commit -m "research: record DeepSeek challenge 003 observations"
git push origin research/epistemic-challenge-002-20261115

unset DEEPSEEK_API_KEY
```
