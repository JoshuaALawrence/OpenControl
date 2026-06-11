# Security Policy

OpenControl gives an MCP client controlled access to a Windows desktop. Please report security issues privately so users have time to update before details are public.

## Supported Versions

OpenControl is pre-1.0. Security fixes are made on `main` and included in the next release.

| Version | Supported |
| --- | --- |
| `main` | Yes |
| Latest release | Yes |
| Older releases | No |

## Reporting a Vulnerability

Do not open a public issue for a vulnerability.

Email reports to `joshua@lawrence.zip`. If GitHub private vulnerability reporting is enabled for this repository, you may also use the repository's Security tab to create a private advisory report.

Please include as much of the following as you can safely share:

- Affected version, commit, or release asset
- Windows version and MCP host, if relevant
- Impact and who can trigger it
- Reproduction steps or proof of concept
- Logs, screenshots, or command output with secrets removed
- Whether the issue is already public or known elsewhere

You should receive an initial response within 7 days. After confirmation, the maintainer will coordinate a fix, release timing, credit, and disclosure details.

## In Scope

Examples of issues this project wants reported privately include:

- Bypass of the application blocklist, screenshot redaction, OCR redaction, or control refusal boundaries
- Exposure of screenshots, OCR text, clipboard contents, file contents, command output, or other local data beyond the requested tool result
- Incorrect targeting that could control a blocked or unintended window
- Command execution, file access, process control, or launch behavior that exceeds the documented tool contract
- Unsafe handling of untrusted tool parameters
- Supply-chain or release artifact issues
- Dependency vulnerabilities with a practical impact on OpenControl users

## Out of Scope

The following are usually not treated as security vulnerabilities:

- Reports that require already-authorized local desktop access and do not cross a documented boundary
- General concerns about using desktop automation or MCP tools without a concrete exploit path
- Vulnerabilities in an MCP host, Windows, or third-party application unless OpenControl creates or amplifies the issue
- Missing hardening features that are already documented as unsupported

## Safe Harbor

Good-faith security research is welcome when it avoids privacy harm and service disruption. Do not access, modify, delete, or disclose data that is not yours. Stop testing and report promptly if you encounter sensitive data or behavior that could harm users.
