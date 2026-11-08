# Niccolò v0.15 — first-party Situation shell

## Goal

Expose the approved three entry intentions through one deliberately small conversational Situation
surface without pretending that unfinished backend/model behavior already exists.

## Implemented

- Axum serves `/`, `/app.js`, `/state.js`, and `/styles.css` from compile-time embedded assets.
- One page exposes all three intentions:
  1. I am about to say or do something.
  2. I am trying to understand something.
  3. Help me figure out what matters.
- A user can set current focus, add user-reported context, add an explicit WHAT-IF premise, retract a
  premise, and see a browser-side revision timeline.
- File selection is deliberately catalogue-only. The page states that filenames are not uploaded or
  analyzed by this milestone.
- The state reducer retains the `hypothetical` kind across later state changes.
- Static assets and API responses share no-store/nosniff/no-referrer/frame-deny headers plus a
  restrictive Content-Security-Policy.
- Browser state semantics are tested using Node's dependency-free built-in test runner.

## Deliberate non-goals

This is not the final visual design, a model chat client, source uploader, persistent Situation store,
identity workflow, or complete source coverage UI. No model call is made. No file contents leave the
browser through this shell. Backend connection comes only after the Situation persistence/API
contract is implemented and tested.

## Offline dependency finding

The prefetched npm package set is useful but its supplied `package-lock.json` is not currently
self-consistent under npm 11.6.1 (`ajv` dependency resolution mismatch). v0.15 therefore does not
silently repair it using network access. The production frontend dependency choice remains open.
