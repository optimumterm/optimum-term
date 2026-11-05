# Security

## Reporting a vulnerability

Please report security problems privately through GitHub: open the [Security tab](../../security) of this repository and click **Report a vulnerability**. Don't open a public issue for them.

You'll get an answer as soon as possible. Once a fix is released, the advisory is published with credit to you, unless you'd rather stay anonymous.

## Supported versions

Only the latest release is supported. Older releases are removed when a new one comes out, so updating to the latest release is how fixes reach you.

## What Optimum does and doesn't do

- It makes no network connections: no telemetry, no crash reports, no update checks. CI enforces that no networking or telemetry crate enters the dependency tree (`deny.toml`).
- It runs one program: `wsl.exe` on Windows, your login shell on macOS.
- It reads and writes only its own config file (`%APPDATA%\Optimum\config.toml` or `~/Library/Application Support/Optimum/config.toml`).

## Verifying a download

Every release file is built by GitHub Actions from the tagged commit and comes with a signed build provenance attestation. Check that a file you downloaded was built by this repository's release workflow:

```
gh attestation verify Optimum.exe --repo optimumterm/optimum-term
gh attestation verify Optimum.dmg --repo optimumterm/optimum-term
```

Each release also has an SPDX SBOM (`Optimum.spdx.json`) listing every dependency, attested for both files, and a `.sha256` checksum for each.
