# Security Policy

## Reporting a vulnerability

If you discover a security issue, please open a private report via GitHub's
[Security Advisories](https://github.com/RobertWsp/github-status/security/advisories/new)
rather than a public issue. We'll acknowledge it as soon as possible.

## How `ghs` handles your GitHub token

`ghs` needs a GitHub token only to query the Actions API. It is designed to
**never persist or transmit your token anywhere except GitHub's API**:

- **Resolution order:** `GITHUB_TOKEN` → `GH_TOKEN` → `settings.token` in your
  config → `gh auth token` (the official GitHub CLI). The first match wins.
- **Preferred source:** environment variable or the GitHub CLI keyring. Storing
  the token in the config file is supported but **not recommended**.
- **No logging of secrets:** the token is never written to logs. Application
  logs go to a local file (`…/logs/ghs.log`) and contain no credentials.
- **No telemetry:** `ghs` makes network requests **only** to
  `https://api.github.com`. There is no analytics, crash reporting, or
  phone-home of any kind.

## Recommended token setup

Use a **fine-grained personal access token** scoped to read-only:

- Repository access: only the repos you want to monitor.
- Permissions: **Actions → Read-only** (and **Contents → Read-only** if needed).

Or simply authenticate the GitHub CLI (`gh auth login`) and let `ghs` pick the
token up automatically — nothing is stored by `ghs` itself in that case.

## If you accidentally committed a token

Treat it as compromised: **revoke it immediately** at
<https://github.com/settings/tokens>, then issue a new one. Do not store tokens
in the repository's `config.toml`.
