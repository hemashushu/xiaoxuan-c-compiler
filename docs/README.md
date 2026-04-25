# XiaoXuan C Compiler

ANCC is a long-term effort to build a deterministic and self-contained C toolchain:

- C compiler
- build system
- C standard library

The project favors reproducibility, strict standard behavior, and portability over platform-specific shortcuts.

## Why this project exists

Most C toolchains accumulate historical behavior, host dependencies, and implementation-defined quirks. ANCC explores a different direction:

- make builds reproducible by default
- reduce hidden host/toolchain coupling
- keep the implementation small, understandable, and auditable
- align with modern C (target: C23)

## Core principles

- **Deterministic by design**
  Same source + same options should produce the same results.

- **Cross-platform first**
  Behavior should remain stable across environments, with minimal reliance on host-specific components.

- **Self-contained stack**
  Compiler, build logic, and libc are designed together for consistent semantics.

- **Standard-oriented**
  Prioritize C standard conformance and avoid non-essential extensions.
