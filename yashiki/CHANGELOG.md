# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.9.6](https://github.com/typester/yashiki/compare/yashiki-v0.9.5...yashiki-v0.9.6) - 2026-01-27

### Fixed

- protect managed windows during native fullscreen transition ([#102](https://github.com/typester/yashiki/pull/102))

## [0.9.5](https://github.com/typester/yashiki/compare/yashiki-v0.9.4...yashiki-v0.9.5) - 2026-01-27

### Fixed

- remove ghost windows on app termination ([#99](https://github.com/typester/yashiki/pull/99))

## [0.9.4](https://github.com/typester/yashiki/compare/yashiki-v0.9.3...yashiki-v0.9.4) - 2026-01-25

### Fixed

- prevent incorrect window deletion when AX API is inaccessible ([#97](https://github.com/typester/yashiki/pull/97))

## [0.9.3](https://github.com/typester/yashiki/compare/yashiki-v0.9.2...yashiki-v0.9.3) - 2026-01-23

### Fixed

- ensure rules are applied and retile happens for all window sync paths ([#92](https://github.com/typester/yashiki/pull/92))

## [0.9.2](https://github.com/typester/yashiki/compare/yashiki-v0.9.1...yashiki-v0.9.2) - 2026-01-23

### Fixed

- detect windows from apps running before yashiki started ([#90](https://github.com/typester/yashiki/pull/90))

## [0.9.1](https://github.com/typester/yashiki/compare/yashiki-v0.9.0...yashiki-v0.9.1) - 2026-01-23

### Added

- add output_id to list-windows output ([#87](https://github.com/typester/yashiki/pull/87))

### Fixed

- use per-window hide position calculation with window size offset ([#86](https://github.com/typester/yashiki/pull/86))

## [0.9.0](https://github.com/typester/yashiki/compare/yashiki-v0.8.3...yashiki-v0.9.0) - 2026-01-22

### Fixed

- track orphaned windows during sleep/wake to restore them to original display ([#84](https://github.com/typester/yashiki/pull/84))
- [**breaking**] use per-display hide position to prevent window disappearance during cross-display fullscreen ([#83](https://github.com/typester/yashiki/pull/83))

## [0.8.3](https://github.com/typester/yashiki/compare/yashiki-v0.8.2...yashiki-v0.8.3) - 2026-01-21

### Fixed

- Filter out Control Center windows early in sync process ([#81](https://github.com/typester/yashiki/pull/81))
- output-send window visibility bug ([#80](https://github.com/typester/yashiki/pull/80))

## [0.8.2](https://github.com/typester/yashiki/compare/yashiki-v0.8.1...yashiki-v0.8.2) - 2026-01-21

### Fixed

- improve handling of windows not in state and hidden window movement ([#78](https://github.com/typester/yashiki/pull/78))

## [0.8.1](https://github.com/typester/yashiki/compare/yashiki-v0.8.0...yashiki-v0.8.1) - 2026-01-21

### Fixed

- prevent visible windows from moving to newly connected displays ([#75](https://github.com/typester/yashiki/pull/75))

### Other

- Split god classes into focused modules ([#76](https://github.com/typester/yashiki/pull/76))

## [0.8.0](https://github.com/typester/yashiki/compare/yashiki-v0.7.7...yashiki-v0.8.0) - 2026-01-21

### Added

- [**breaking**] Remove polling threads, use event-driven CFRunLoopSource signaling ([#73](https://github.com/typester/yashiki/pull/73))

## [0.7.7](https://github.com/typester/yashiki/compare/yashiki-v0.7.6...yashiki-v0.7.7) - 2026-01-20

### Fixed

- Hide windows outside bounding box of all monitors ([#71](https://github.com/typester/yashiki/pull/71))

## [0.7.6](https://github.com/typester/yashiki/compare/yashiki-v0.7.5...yashiki-v0.7.6) - 2026-01-20

### Fixed

- Fix focus state inconsistencies in tag operations and window lifecycle ([#69](https://github.com/typester/yashiki/pull/69))
- version cmd ([#68](https://github.com/typester/yashiki/pull/68))

## [0.7.5](https://github.com/typester/yashiki/compare/yashiki-v0.7.4...yashiki-v0.7.5) - 2026-01-20

### Added

- add --track option to exec command for process lifecycle management ([#66](https://github.com/typester/yashiki/pull/66))

## [0.7.4](https://github.com/typester/yashiki/compare/yashiki-v0.7.3...yashiki-v0.7.4) - 2026-01-20

### Added

- auto-recover event tap when disabled by macOS ([#63](https://github.com/typester/yashiki/pull/63))

## [0.7.3](https://github.com/typester/yashiki/compare/yashiki-v0.7.2...yashiki-v0.7.3) - 2026-01-20

### Fixed

- display size change issue ([#60](https://github.com/typester/yashiki/pull/60))
- raycast focus issue ([#59](https://github.com/typester/yashiki/pull/59))

### Other

- release v0.7.2 ([#58](https://github.com/typester/yashiki/pull/58))

## [0.7.2](https://github.com/typester/yashiki/compare/yashiki-v0.7.1...yashiki-v0.7.2) - 2026-01-20

### Fixed

- display size change issue ([#60](https://github.com/typester/yashiki/pull/60))
- raycast focus issue ([#59](https://github.com/typester/yashiki/pull/59))

## [0.7.1](https://github.com/typester/yashiki/compare/yashiki-v0.7.0...yashiki-v0.7.1) - 2026-01-20

### Fixed

- Multi-monitor display handling improvements ([#57](https://github.com/typester/yashiki/pull/57))

## [0.7.0](https://github.com/typester/yashiki/compare/yashiki-v0.6.0...yashiki-v0.7.0) - 2026-01-19

### Added

- support "none" matcher for --ax-id and --subrole ([#55](https://github.com/typester/yashiki/pull/55))
- [**breaking**] manage non-normal layer windows as floating by default ([#53](https://github.com/typester/yashiki/pull/53))

### Fixed

- apply rules immediately on rule-add after init completed ([#54](https://github.com/typester/yashiki/pull/54))

## [0.6.0](https://github.com/typester/yashiki/compare/yashiki-v0.5.4...yashiki-v0.6.0) - 2026-01-19

### Added

- add --all and --debug option to list-windows ([#48](https://github.com/typester/yashiki/pull/48))

### Fixed

- remove duplicate codes ([#50](https://github.com/typester/yashiki/pull/50))
- focus cycling and new window tag assignment ([#49](https://github.com/typester/yashiki/pull/49))
- apply ignore rules and fetch ax attributes for initial windows ([#47](https://github.com/typester/yashiki/pull/47))
- Apply "first match wins" logic to Float/NoFloat rules ([#46](https://github.com/typester/yashiki/pull/46))

## [0.5.4](https://github.com/typester/yashiki/compare/yashiki-v0.5.3...yashiki-v0.5.4) - 2026-01-19

### Added

- window-swap ([#44](https://github.com/typester/yashiki/pull/44))

## [0.5.3](https://github.com/typester/yashiki/compare/yashiki-v0.5.2...yashiki-v0.5.3) - 2026-01-19

### Fixed

- detect windows from apps running without windows at startup ([#40](https://github.com/typester/yashiki/pull/40))

## [0.5.2](https://github.com/typester/yashiki/compare/yashiki-v0.5.1...yashiki-v0.5.2) - 2026-01-19

### Added

- eliminate hotkey processing latency with CFRunLoopSource ([#38](https://github.com/typester/yashiki/pull/38))

## [0.5.1](https://github.com/typester/yashiki/compare/yashiki-v0.5.0...yashiki-v0.5.1) - 2026-01-19

### Added

- more matchers! ([#36](https://github.com/typester/yashiki/pull/36))

### Fixed

- filter out non-normal windows ([#34](https://github.com/typester/yashiki/pull/34))

## [0.5.0](https://github.com/typester/yashiki/compare/yashiki-v0.4.1...yashiki-v0.5.0) - 2026-01-18

### Added

- move outer-gap to core ([#31](https://github.com/typester/yashiki/pull/31))
- add state streaming for external tools ([#30](https://github.com/typester/yashiki/pull/30))
- add cursor warp (mouse follows focus) ([#29](https://github.com/typester/yashiki/pull/29))

## [0.4.1](https://github.com/typester/yashiki/compare/yashiki-v0.4.0...yashiki-v0.4.1) - 2026-01-18

### Fixed

- use Tag::from_mask() for rule tag application ([#27](https://github.com/typester/yashiki/pull/27))

## [0.4.0](https://github.com/typester/yashiki/compare/yashiki-v0.3.0...yashiki-v0.4.0) - 2026-01-18

### Added

- add window-close cmd ([#24](https://github.com/typester/yashiki/pull/24))
- add window-toggle-float ([#23](https://github.com/typester/yashiki/pull/23))

### Fixed

- fix the issue where rules doesn't apply correct timing ([#22](https://github.com/typester/yashiki/pull/22))

## [0.3.0](https://github.com/typester/yashiki/compare/yashiki-v0.2.0...yashiki-v0.3.0) - 2026-01-18

### Added

- Window Rules + Fullscreen support ([#19](https://github.com/typester/yashiki/pull/19))

## [0.2.0](https://github.com/typester/yashiki/compare/yashiki-v0.1.1...yashiki-v0.2.0) - 2026-01-18

### Added

- add exec-path related command ([#17](https://github.com/typester/yashiki/pull/17))
- create test workflow ([#15](https://github.com/typester/yashiki/pull/15))

## [0.1.1](https://github.com/typester/yashiki/compare/yashiki-v0.1.0...yashiki-v0.1.1) - 2026-01-18

### Fixed

- ensure yashiki command is available in init script ([#12](https://github.com/typester/yashiki/pull/12))

## [0.1.0](https://github.com/typester/yashiki/releases/tag/v0.1.0) - 2026-01-18

### Fixed

- fixed the issue where state didn't update when apps is terminated
- fix toggle tag issue
- fix several layout issues and support gap settings
- fix several layout issues
- fix initial layout issue

### Other

- app bundle workflow
- add --layout option to layout-cmd
- runloop optimization
- use argh for subcommand args
- command restructure
- output related upgrade
- layout switch capability
- byobu fix
- byobu layout
- cleanup build
- test upgrade
- test functionality
- add tag switching when external focus change is happened
- add yashiki-layout- prefix to layout command
- exec command
- improve focus window detection
- inc/dec-main, zoom
- view-tag-last
- multi monitor support
- auto retile
- focus window
- config and hotkey
- initial layout
- tag worksplace
- IPC
- window states
- window observer
- runloop and tokio setup
- testing move window
- list windows
- initial commit
