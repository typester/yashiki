# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.12.1](https://github.com/typester/yashiki/compare/yashiki-v0.12.0...yashiki-v0.12.1) - 2026-04-13

### Fixed

- suppress cross-PID spurious focus bounce on tag switch ([#165](https://github.com/typester/yashiki/pull/165))
- suppress unintended tag switch from macOS auto-activation after app termination ([#161](https://github.com/typester/yashiki/pull/161))

## [0.12.0](https://github.com/typester/yashiki/compare/yashiki-v0.11.6...yashiki-v0.12.0) - 2026-02-25

### Added

- [**breaking**] add keybinding mode system (river-style) ([#157](https://github.com/typester/yashiki/pull/157))
- update focused_display via auto-raise when cursor moves to different display ([#156](https://github.com/typester/yashiki/pull/156))

### Fixed

- call activate_application for already-frontmost apps to fix keyboard focus ([#155](https://github.com/typester/yashiki/pull/155))

## [0.11.6](https://github.com/typester/yashiki/compare/yashiki-v0.11.5...yashiki-v0.11.6) - 2026-02-20

### Fixed

- prevent auto-raise from focusing ignored windows ([#152](https://github.com/typester/yashiki/pull/152))

## [0.11.5](https://github.com/typester/yashiki/compare/yashiki-v0.11.4...yashiki-v0.11.5) - 2026-02-19

### Fixed

- make window-focus next/prev follow visual layout order instead of WindowId order ([#150](https://github.com/typester/yashiki/pull/150))

## [0.11.4](https://github.com/typester/yashiki/compare/yashiki-v0.11.3...yashiki-v0.11.4) - 2026-02-17

### Added

- suppress auto-raise while popup menus are visible ([#146](https://github.com/typester/yashiki/pull/146))

### Fixed

- unify event emission across all callbacks using capture/emit pattern ([#144](https://github.com/typester/yashiki/pull/144))

## [0.11.3](https://github.com/typester/yashiki/compare/yashiki-v0.11.2...yashiki-v0.11.3) - 2026-02-16

### Fixed

- emit DisplayFocused event on auto-raise and external focus change ([#142](https://github.com/typester/yashiki/pull/142))

## [0.11.2](https://github.com/typester/yashiki/compare/yashiki-v0.11.1...yashiki-v0.11.2) - 2026-02-15

### Fixed

- emit missing events on display connect/disconnect and scale bar properties ([#139](https://github.com/typester/yashiki/pull/139))

## [0.11.1](https://github.com/typester/yashiki/compare/yashiki-v0.11.0...yashiki-v0.11.1) - 2026-02-15

### Fixed

- wake up RunLoop after snapshot request to fix subscribe --snapshot ([#134](https://github.com/typester/yashiki/pull/134))

## [0.11.0](https://github.com/typester/yashiki/compare/yashiki-v0.10.7...yashiki-v0.11.0) - 2026-02-10

### Fixed

- [**breaking**] rules without --window-level should only match normal-layer windows ([#132](https://github.com/typester/yashiki/pull/132))

## [0.10.7](https://github.com/typester/yashiki/compare/yashiki-v0.10.6...yashiki-v0.10.7) - 2026-02-08

### Added

- add file-based logging with runtime log level control ([#130](https://github.com/typester/yashiki/pull/130))

### Fixed

- use AXFrontmost instead of activate_application to prevent cross-display focus redirect ([#129](https://github.com/typester/yashiki/pull/129))
- retry observer registration on AppLaunched and fix CallbackContext leak ([#128](https://github.com/typester/yashiki/pull/128))

## [0.10.6](https://github.com/typester/yashiki/compare/yashiki-v0.10.5...yashiki-v0.10.6) - 2026-02-08

### Added

- add cross-display focus redirect suppression ([#121](https://github.com/typester/yashiki/pull/121))

### Fixed

- remove ghost windows from dead processes without AppTerminated notification ([#126](https://github.com/typester/yashiki/pull/126))
- exclude non-normal layer windows from transition guard to prevent ghost windows ([#125](https://github.com/typester/yashiki/pull/125))
- revert focused_display on spurious focus redirect to prevent permanent state corruption ([#123](https://github.com/typester/yashiki/pull/123))
- use effective_focused_display() to protect all commands from spurious focus redirect ([#122](https://github.com/typester/yashiki/pull/122))

## [0.10.5](https://github.com/typester/yashiki/compare/yashiki-v0.10.4...yashiki-v0.10.5) - 2026-02-05

### Fixed

- update window frame in State after retile ([#119](https://github.com/typester/yashiki/pull/119))

## [0.10.4](https://github.com/typester/yashiki/compare/yashiki-v0.10.3...yashiki-v0.10.4) - 2026-02-03

### Other

- Refactor attribute handling to centralize CFString runtime checks in … ([#114](https://github.com/typester/yashiki/pull/114))

## [0.10.3](https://github.com/typester/yashiki/compare/yashiki-v0.10.2...yashiki-v0.10.3) - 2026-02-02

### Fixed

- skip AX check for non-normal layer windows in removal logic ([#116](https://github.com/typester/yashiki/pull/116))

## [0.10.2](https://github.com/typester/yashiki/compare/yashiki-v0.10.1...yashiki-v0.10.2) - 2026-01-28

### Other

- integrate tag-view sync with sync_helper pattern ([#111](https://github.com/typester/yashiki/pull/111))

## [0.10.1](https://github.com/typester/yashiki/compare/yashiki-v0.10.0...yashiki-v0.10.1) - 2026-01-28

### Added

- add auto-raise (focus follows mouse) feature ([#109](https://github.com/typester/yashiki/pull/109))

## [0.10.0](https://github.com/typester/yashiki/compare/yashiki-v0.9.7...yashiki-v0.10.0) - 2026-01-28

### Added

- [**breaking**] output-send keeps focus on source display (River-style) ([#106](https://github.com/typester/yashiki/pull/106))

### Fixed

- suppress spurious focus changes for multi-window apps ([#107](https://github.com/typester/yashiki/pull/107))

## [0.9.7](https://github.com/typester/yashiki/compare/yashiki-v0.9.6...yashiki-v0.9.7) - 2026-01-28

### Fixed

- prevent window migration between displays on sleep/wake ([#104](https://github.com/typester/yashiki/pull/104))

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
