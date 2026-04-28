# XiaoXuan C Compiler

A clean-room C language toolchain for the AI coding era.

ANCC (XiaoXuan C Compiler) is a long-term effort to build a complete, self-contained C toolchain — compiler, build system, and standard library — designed from first principles for deterministic behavior and strict standard conformance.

The architecture prioritizes explicit semantics, reproducibility, and low hidden coupling so machine-assisted development remains auditable and trustworthy.

## Motivation

Most C toolchains carry decades of accumulated behavior: platform quirks, implicit host dependencies, and undefined-behavior silently swept under the rug. ANCC takes a different approach: start from a clean slate, stay close to the standard, and make reproducibility a first-class property rather than an afterthought.

## What it includes

| Component        | Description                                                   |
|------------------|---------------------------------------------------------------|
| Compiler         | A C23 compiler targeting multiple architectures               |
| Build system     | Reproducible, host-independent build orchestration            |
| Standard library | A libc co-designed with the compiler for consistent semantics |

## Design principles

- Deterministic
  Given the same source and options, ANCC always produces the same output — byte-for-byte — regardless of when or where it runs.
- Self-contained
  The compiler, build logic, and standard library are designed as a unit. There are no hidden dependencies on the host toolchain.
- Standard-first
  Conformance to C23 (ISO/IEC 9899:2024) is the primary correctness criterion. Non-standard extensions are avoided unless essential.
- Auditable
  The implementation is intentionally small and readable. No magic, no sprawling macro forests — just clear code that can be understood and verified.

## License

Mozilla Public License 2.0 with additional terms.
See [LICENSE](LICENSE), [LICENSE.additional](LICENSE.additional), and [CONTRIBUTING](CONTRIBUTING) for details.
