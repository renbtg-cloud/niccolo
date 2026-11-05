# v0.30 — Persistent first-party web shell

The browser can explicitly connect to the loopback development API and persist Situation focus and attributed human context. Credentials are held only in page memory. The UI renders server-owned immutable revision history after connection.

Boundaries remain explicit: intention selection is session steering, file selection only records local filenames, no file bytes are uploaded by this milestone, and no LLM analysis is performed. The browser therefore cannot label an unuploaded source as analyzed.

A real TCP integration test serves the Axum router on `127.0.0.1`, fetches the shell and API module, creates a persistent Situation through authenticated HTTP, adds a hypothetical contribution, and reads it back.
