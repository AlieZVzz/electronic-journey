# Electronic Journey repository guide

## Project purpose

Electronic Journey is a privacy-first Windows and macOS desktop application
for recording a user's own digital journey. Captures must be visible,
consensual, locally encrypted before upload, and recoverable after interruption.

Core goals:

- Build one desktop product with Tauri 2, React, TypeScript, Vite, and Rust.
- Keep screenshot, key, filesystem, database, and network access behind narrow
  Rust interfaces and Tauri commands.
- Ensure the service and object store only receive authenticated ciphertext.

## Technology

- Desktop: Tauri 2 with Rust.
- UI: React 19, TypeScript, and Vite 6.
- Local data: SQLite through `sqlx`.
- Crypto: XChaCha20-Poly1305 envelope-encryption building blocks.
- API: Rust Axum; future metadata store is PostgreSQL.
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
- Treat authentication, encryption, permissions, data deletion, migrations,
  update signing, and recovery behavior as security-sensitive. Propose the
  design and test strategy before changing them.
- Do not claim a screenshot, upload, deletion, or recovery operation succeeded
  until its documented verification step has passed.
- Add focused tests for new business logic and regression tests for bugs.
- Run the smallest relevant checks, then `scripts/check.sh` when practical.
- Finish with the files changed, checks run, and any remaining risk.

## Security rules

- Never read, log, print, or commit secrets, tokens, private keys, recovery
  keys, authorization headers, presigned URLs, screenshots, or plaintext
  thumbnail data.
- Never add analytics, remote fonts, remote images, or new outbound requests
  without an explicit product need and privacy review.
- Never allow the frontend arbitrary filesystem, shell, screenshot, or network
  access. Use minimal Tauri capabilities and validated commands.
- Do not silently weaken TLS, AEAD authentication, key storage, update
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
