# Security Policy

NEX is experimental software. Do not use it for security-sensitive production
workloads yet.

## Reporting

Please report security issues privately to the maintainers. Include:

- Affected version or commit.
- Reproduction steps.
- Expected and actual behavior.
- Impact assessment.

## Current security status

- Unsafe code is not yet part of the implemented language.
- Dependency auditing and fuzzing are planned but not complete.
- Generated programs are native executables and inherit the privileges of the
  user running them.
- Do not run untrusted `.nex` programs without review.
