# DragonForge Asset Vault Security Policy

DragonForge Asset Vault manages project assets, metadata, storage, search, and collaboration workflows. Security-sensitive findings must not be disclosed in ordinary public issues.

## Current status

The repository is an active maintenance/development baseline around the v0.18.2 / Phase 18.2 line recorded in project history. It is not independently security audited or certified, and public binary redistribution remains subject to the unresolved dependency-license gate documented by the DragonForge public-readiness program.

## Reporting a vulnerability

Use GitHub's private security-advisory reporting flow when available:

`https://github.com/djames1987/DragonForge-Asset-Vault/security/advisories/new`

If that private flow is unavailable, contact the repository owner through a private GitHub channel. Do **not** publish exploit details in a public issue.

A useful private report includes the affected commit/version, component, reproduction steps with synthetic data, expected versus observed behavior, impact, and sanitized evidence.

Do not include credentials, API tokens, private keys, proprietary game assets, licensed third-party assets, customer/client material, personal data, private project repositories, or sensitive logs in public reports or attachments.

## Scope

Examples of security-relevant findings include authorization or access-control failures, path traversal, unsafe archive/import behavior, storage-boundary escapes, credential/token exposure, insecure project sharing, tampering with integrity/version metadata, or unintended disclosure through search/indexing.

Only test systems, repositories, and assets you own or are explicitly authorized to test.