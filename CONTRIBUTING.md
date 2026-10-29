# Contributing to AInonymous

AInonymous is alpha research software. These rules keep the security posture and
the review trail honest; they are deliberately short.

## Ground rules

- The documentation policy in [docs/README.md](docs/README.md) applies: source
  code is authoritative, and no claim of anonymity, zero-knowledge inference,
  public-network Sybil resistance, production SD-WAN integration or native
  multi-node llama.cpp execution may be presented as complete without tests.
- A security-sensitive change (identity, admission, transport, release
  verification) must come with a test that fails without the change.
- Never commit secrets, keystore material or logs containing them.

## Branches and commits

- Branch from `main`: `fix/<topic>`, `feat/<topic>`, `docs/<topic>`, `chore/<topic>`.
- Commit subjects are imperative, preferably `type(scope): summary`
  (`fix(quic): ...`, `docs(security): ...`).
- One pull request, one concern. Split mechanical reformatting from behaviour changes.

## Review policy for large pull requests

CodeQL is not a review. Above roughly 1,000 changed lines, a pull request needs
either a second human reviewer or a written self-review checklist in its
description:

- [ ] Every new security-relevant path has a negative test.
- [ ] Pinned versions, DNA hashes and checksums are unchanged, or bumped on purpose.
- [ ] Documentation states what is proven and what is not.
- [ ] CI is green on the final commit, not only on an earlier one.

## Checks before opening a pull request

```bash
cargo check --locked --workspace
cargo test --locked --workspace
cargo clippy --locked --workspace -- -A dead_code -A unused
python scripts/hybridnode/validate_config.py \
  hybridnode/configs/ainonymous.hybridnode.yaml \
  hybridnode/configs/generic-project.hybridnode.yaml
python -m pytest examples/pytorch/tests -q   # requires torch
```

Bundles, conductor-configuration and container checks run in CI
(`.github/workflows/ci.yml`); Holochain 0.7.0 `hc` plus `holochain` are required
for the bundle job locally (`bash scripts/build-happ.sh release`).

## Security reports

Follow [docs/SECURITY.md](docs/SECURITY.md): do not publish live credentials or
exploit details in a public issue; contact the repository owner privately before
coordinated disclosure.
