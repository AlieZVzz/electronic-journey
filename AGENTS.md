# Electronic Journey repository guide

## Project purpose

Electronic Journey is a local-first Windows and macOS desktop application
for recording a user's own digital journey. Captures must be visible,
consensual, stored locally, and recoverable after interruption. Images leave
the device only when the user explicitly selects them and confirms an SFTP
upload from the Rust client to a configured personal server folder.

Core goals:

- Build one desktop product with Tauri 2, React, TypeScript, Vite, and Rust.
- Keep screenshot, key, filesystem, database, and network access behind narrow
  Rust interfaces and Tauri commands.
- Do not build or use a first-party image cloud, object store, or LLM proxy.
- Keep SSH credentials and image network access behind narrow Rust interfaces.
- Do not make the client aware of Hermes, prompts, models, or remote consumers.

## Technology

- Desktop: Tauri 2 with Rust.
- UI: React 19, TypeScript, and Vite 6.
- Local data: SQLite through `sqlx`.
- Local images: lossless WebP originals and bounded WebP thumbnails.
- Remote upload: Rust SFTP with pinned host fingerprints and key authentication.
- Package manager: npm.

## Commands

- Install frontend dependencies: `npm install`
- Browser UI: `npm run dev`
- Desktop UI: `npm run dev:desktop`
- Frontend checks: `npm run check`
- Rust checks: `cargo check --workspace`
- Rust tests: `cargo test --workspace`
- All available local checks: `scripts/check.sh`

Rust and platform development prerequisites are listed in `README.md`.

## Work protocol

- Read the relevant module and `electronic-journey-design.md` before changing
  behavior.
- Prefer small scoped changes and preserve public interfaces unless the task
  explicitly changes them.
- Treat authentication, credentials, permissions, data
  deletion, migrations, update signing, and SFTP upload behavior as
  security-sensitive. Propose the design and test strategy before changing
  them.
- Do not claim a screenshot, upload, deletion, or recovery operation succeeded
  until its documented verification step has passed.
- Add focused tests for new business logic and regression tests for bugs.
- Run the smallest relevant checks, then `scripts/check.sh` when practical.
- Finish with the files changed, checks run, and any remaining risk.

## Security rules

- Never read, log, print, or commit secrets, tokens, private keys,
  authorization headers, screenshots, prompts, full model answers, or
  plaintext thumbnail data.
- Never add analytics, remote fonts, remote images, or new outbound requests
  except a user-initiated SSH/SFTP action covered by the product design and
  privacy review.
- Never allow the frontend arbitrary filesystem, shell, screenshot, or network
  access. Use minimal Tauri capabilities and validated commands.
- Do not silently weaken TLS, key storage, update
  signatures, or deletion confirmation.
- Do not execute deployment, publishing, remote push, destructive Git, or bulk
  deletion unless the user explicitly asks.

## References

- Product and security design: `electronic-journey-design.md`
- Architecture: `docs/architecture.md`
- Threat model: `docs/threat-model.md`
- File format: `docs/file-format.md`
- API contract: `docs/api.md`
- Coding conventions: `docs/coding-style.md`
- Review guide: `docs/code-review.md`
- Current tasks: `tasks/todo.md` and `tasks/in-progress.md`
- Project decisions: `memory/decisions.md`
- Lessons learned: `memory/lessons-learned.md`
