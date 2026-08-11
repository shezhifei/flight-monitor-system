# Security Policy

## Supported Versions

This project is in pre-release open-source preparation. Security fixes should
target the default branch unless a maintained release branch is announced.

## Reporting a Vulnerability

Do not disclose exploitable vulnerabilities in public issues before the
maintainers have had time to assess and remediate them.

Until a dedicated security contact is published, report vulnerabilities to the
project maintainers through the private channel used by the repository owner.
Include:

- affected component and version or commit;
- reproduction steps;
- expected and observed impact;
- whether credentials, personal data, or operational data may be exposed;
- any suggested mitigation.

## Secret Handling

- Do not commit `.env`, rendered Vault output, AppRole credentials, TLS private
  keys, database dumps, production logs, or generated runtime state.
- Example files must use placeholders or local-only demo values.
- Runtime secrets should be delivered through Vault or environment-specific
  secure injection, not hard-coded in source or deployment manifests.
