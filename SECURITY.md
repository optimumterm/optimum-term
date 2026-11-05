# Security policy

## Reporting a vulnerability

Report security vulnerabilities privately through GitHub's private vulnerability reporting:

**https://github.com/optimumterm/optimum-term/security/advisories/new**

Please don't open a public issue, discussion or pull request for a vulnerability. Include what you found, how to reproduce it, which version and platform you used, and what an attacker could do with it.

## What happens next

- You get a first answer within **7 days**.
- Once the problem is confirmed, we work on a fix in a private security advisory and keep you posted on progress. You're welcome to collaborate on the fix there.
- A fixed release is published as soon as the fix is ready, usually within **30 days**, at most **90 days** after the report.
- The advisory is then disclosed publicly with a CVE where applicable, crediting you unless you'd rather stay anonymous.

If you don't hear back within 7 days, please comment on your report again.

## Supported versions

Only the latest release is supported and receives security fixes. Older releases are removed when a new one comes out, so updating to the latest release is how fixes reach you.

## Scope

Optimum is a terminal emulator. In scope are problems in Optimum itself, for example:

- escape sequences or pasted text that make Optimum execute commands, write files or leak data;
- crashes or memory corruption triggered by program output;
- problems in the installer, the release pipeline or the published files.

Bugs in the programs you run inside Optimum (your shell, WSL, tmux) belong to those projects. Problems in the terminal core libghostty-vt are reported upstream to [Ghostty](https://github.com/ghostty-org/ghostty/security) as well.

## What Optimum does and doesn't do

- It makes no network connections: no telemetry, no crash reports, no update checks. CI enforces that no networking, TLS or telemetry crate enters the dependency tree ([`deny.toml`](deny.toml)).
- It runs one program: `wsl.exe` on Windows, your login shell on macOS.
- It reads and writes only its own config file (`%APPDATA%\Optimum\config.toml` or `~/Library/Application Support/Optimum/config.toml`).

## Verifying a download

Every release file is built by GitHub Actions from the tagged commit and signed with [Sigstore](https://www.sigstore.dev) build provenance. Check that a file you downloaded was built by this repository's release workflow:

```
gh attestation verify Optimum.exe --repo optimumterm/optimum-term
gh attestation verify Optimum.dmg --repo optimumterm/optimum-term
```

The signature bundles are also attached to each release (`Optimum.exe.sigstore.json`, `Optimum.dmg.sigstore.json`), so you can verify without querying GitHub:

```
gh attestation verify Optimum.exe --repo optimumterm/optimum-term --bundle Optimum.exe.sigstore.json
```

Each release also has an SPDX SBOM (`Optimum.spdx.json`) listing every dependency, attested for both files, and a `.sha256` checksum for each file.
