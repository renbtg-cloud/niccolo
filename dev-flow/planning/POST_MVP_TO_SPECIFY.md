# A ESPECIFICAR EM DETALHES PÓS-MVP

This file records deliberately deferred product/architecture ideas. They are **not part of the initial MVP scope** and should not complicate the first release unless a concrete implementation need appears earlier.

## 1. Capability-aware reasoning topology

Niccolò should eventually choose the *reasoning topology* from the capability actually available for a given run, rather than from provider branding alone.

Possible inputs to the decision include:

- model capability / reasoning strength;
- context window and effective token budget available to this run;
- maximum output tokens;
- monetary budget;
- latency/deadline budget;
- structured-output reliability;
- tool-use capability;
- local vs. remote execution;
- privacy / data-egress constraints;
- Situation complexity and evidence volume.

Important principle:

> A very strong model with a small token budget may need the same highly decomposed state-machine topology used for a much smaller model. It remains stronger inside each small step, but the topology is chosen from effective available capability, not model prestige.

Candidate topologies to specify later:

- **Wide / few-call:** large reasoning packets, a small number of broad calls (for example 3–4) when a strong model and generous budget make that sensible.
- **Decomposed:** medium-grained staged reasoning.
- **Highly scaffolded / state-machine:** many narrow, bounded calls with explicit intermediate state and validation, suitable for small/local models or constrained budgets.

The current durable DAG/state-machine path is already a useful foundation for the scaffolded end of this spectrum.

## 2. Model/provider capability metadata

A later specification should define a provider-neutral capability description (working name: `ModelCapabilities`) instead of hard-coding assumptions such as “Claude means powerful” or “Qwen means weak.”

Potential fields include context capacity, structured-output reliability, reasoning strength, latency, cost, tool support, privacy class and local/remote status.

The model is a replaceable reasoning worker. Niccolò's epistemic structure, provenance, Situation state, controller, human-agency boundaries and validation remain the product core.

## 3. User-configurable external LLM / BYOK

Deferred from the initial MVP.

A later product version may allow a technical user to configure one or more external LLM providers and bring their own API keys.

Possible scope:

- provider registry per user and/or workspace;
- model selection;
- multiple provider credentials;
- OpenAI-compatible endpoints such as DeepSeek/OpenRouter/vLLM-style services;
- native provider adapters where worthwhile;
- local/self-hosted endpoints;
- explicit privacy/egress controls;
- secret storage that keeps raw credentials out of reasoning artifacts, logs and normal application data.

The initial MVP does **not** require a user-facing BYOK planner/configuration experience. Server/operator-level external OpenAI-compatible configuration is sufficient for now.

## 4. Multi-provider and per-step routing

Later, Niccolò may select different models for different reasoning steps rather than binding one model to the entire operation.

Examples to investigate:

- local/small model for extraction, classification, normalization or preliminary candidate generation;
- stronger model for synthesis or adversarial challenge;
- provider failover based on availability, budget or policy;
- route selection based on measured capability rather than brand.

This must remain auditable: requested provider/model, effective provider/model, fallback use, token usage, latency and budget consumption should remain inspectable.

## 5. Product principle to preserve

The magic should live in **Niccolò**, not in dependence on one frontier LLM.

A frontier model with abundant budget may produce better/faster results because Niccolò can compress the reasoning workflow into fewer broad calls. A cheaper model such as DeepSeek, or a small self-hosted Qwen, should still be useful because Niccolò can compensate with more decomposition, scaffolding, state, validation and bounded intermediate steps.

This is an architectural direction, **not yet a performance claim**. Any future claim that one topology/provider combination is better must be established empirically with reproducible evaluation.

## 6. MVP boundary

For the initial product, prefer simplicity:

- self-hosted Qwen-first is acceptable;
- external LLM support may remain operator-configured;
- use the existing reasoning style across providers;
- do not delay the MVP for capability-aware routing, user-facing BYOK, multi-provider policy or topology optimization.

These topics are intentionally parked here for detailed specification **after the MVP**.
