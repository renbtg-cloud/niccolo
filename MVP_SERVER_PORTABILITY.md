# MVP server portability — isolated late milestone

Status: **required for the MVP, intentionally deferred** until the Linux-first MVP v1.0 is nearly complete and stable.

## Requirement

Niccolò Server must execute **natively** on all three host operating systems:

- Linux
- Windows
- macOS

The same product/server codebase should be used across platforms, with platform-specific adjustments isolated as much as practical.

## Execution order

1. Complete and stabilize the Linux-first MVP v1.0 functional flow and tests.
2. **Only after v1.0 is almost ready**, begin a separate, bounded portability effort covering native Windows and macOS server builds and runtime validation.
3. Validate server startup, API, PostgreSQL connectivity, analysis execution and persistence/restart on each OS. Do not call portability complete based on compilation alone.
4. Keep cross-platform fixes separate from unrelated MVP feature work so the existing Linux path is not destabilized.

## Explicit non-goals

- Building native iOS/Android applications.
- Desktop GUI wrappers or special OS-specific user experiences.
- Native installers, installers for PostgreSQL, or automated OS provisioning.
- Making Docker, WSL or another VM a mandatory runtime dependency.

## Operator responsibilities

The user/operator installs, starts, configures and administers PostgreSQL independently. Niccolò Server connects to an existing, correctly configured PostgreSQL instance using supplied connection parameters. The frontend remains browser-accessible.

This is a **late MVP acceptance requirement**, not permission to delay core MVP completion with premature cross-platform work.
