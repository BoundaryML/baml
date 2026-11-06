# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.4](https://github.com/Amanieu/corosensei/compare/v0.3.3...v0.3.4) - 2026-05-24

### Added

- Added SEH unwind support for x86_64 UEFI target ([#73](https://github.com/Amanieu/corosensei/pull/73))
- Added SEH unwind support for aarch64 UEFI target ([#75](https://github.com/Amanieu/corosensei/pull/75))
- Made `Coroutine::with_stack_unchecked` public ([#74](https://github.com/Amanieu/corosensei/pull/74))

### Fixed

- Fixed clobbers on PowerPC to include all non-volatile registers ([#77](https://github.com/Amanieu/corosensei/pull/77))

## [0.3.3](https://github.com/Amanieu/corosensei/compare/v0.3.2...v0.3.3) - 2026-02-21

### Other

- Add support for UEFI targets ([#66](https://github.com/Amanieu/corosensei/pull/66))

## [0.3.2](https://github.com/Amanieu/corosensei/compare/v0.3.1...v0.3.2) - 2025-12-03

- Added PowerPC64 support (#61)

## [0.3.1](https://github.com/Amanieu/corosensei/compare/v0.3.0...v0.3.1) - 2025-10-11

- Try harder when forcing coroutines to unwind (#62)

## [0.3.0](https://github.com/Amanieu/corosensei/compare/v0.2.2...v0.3.0) - 2025-09-27

- Re-introduce `ScopedCoroutine` (#58, #60)
- Add support for running with sanitizers (#54)
- Require `Coroutine` stack to be `'static` (#44)
- Fixed linker errors related `stack_init_trampoline_return` (#55)

## [0.2.2](https://github.com/Amanieu/corosensei/compare/v0.2.1...v0.2.2) - 2025-05-27

- Fixed mutable TEB fields not being updated when `force_reset` is called. (#48)

## [0.2.1] - 2024-10-12

- Avoid using `.hidden` for functions declared in `global_asm!` since that may
  cause linker errors.

## [0.2.0] - 2024-10-07

- `ScopedCoroutine` has been removed since it turned out to be unsound. (#28, #36)
- Fixed SEH exception chain not getting correctly reset on x86 Windows when using trap handling. (#33)
- Fixed various warnings and errors from lints. (#33)
- Updated `windows-sys` dependency.

## [0.1.4] - 2023-08-23

- Added support for LoongArch. (#17)

## [0.1.3] - 2022-07-01

- Added support for thumbv6-none-eabi target.
- Fixed trap handling support on i686-windows targets.
- Fixed build on RISC-V targets.

## [0.1.2] - 2022-03-14

- Added the `on_stack` function which provides a simpler but faster alternative to coroutines when only stack switching is needed.
- Changed `on_parent_stack` to use `on_stack` internally.
- Fixed the RISC-V code to use the same frame pointer offset as GCC/LLVM.

## [0.1.1] - 2022-02-25

- Fixed image in README and crate documentation.
- Fixed RISC-V docs build on docs.rs.

## [0.1.0] - 2022-02-25

Initial release.

[unreleased]: https://github.com/Amanieu/corosensei/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/Amanieu/corosensei/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/Amanieu/corosensei/compare/v0.1.4...v0.2.0
[0.1.4]: https://github.com/Amanieu/corosensei/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/Amanieu/corosensei/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/Amanieu/corosensei/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/Amanieu/corosensei/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/Amanieu/corosensei/releases/tag/v0.1.0
