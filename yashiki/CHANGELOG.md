# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
