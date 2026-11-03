# Hostwatch

English · [中文](README.md)

A tiny, read-only Mac menu-bar app that tells you **why** your Mac is slow, and only speaks up when something is actually wrong. Written in Rust with native AppKit: one process, no WebView, no helper processes, about 0.8 MB on disk.

Hostwatch watches memory pressure, swap activity, processor load, disk space and the system's cooling level. Instead of a wall of gauges it gives one plain sentence ("Nothing needs attention") and, when something persists, what is causing it and what you can do.

## What it does

- **Verdict first.** The popover opens on one conclusion, four rows (memory, processor, disk space, temperature) and at most one suggestion. Each row opens a detail page with a real-timestamp trend; gaps stay gaps.
- **Alerts that wait for evidence.** A warning needs 2 abnormal samples in a row, critical states alert at once, recovery needs 2 normal samples, and the same issue is not repeated within 10 minutes. Samples that cannot be read count as neither problem nor recovery. Normal file cache and an old swap file never trigger "clean up" advice.
- **Notifications that name the cause.** From Hostwatch.app, when an issue is raised or gets worse, you get one notification: the issue (for example *"Memory is running low; the Mac may slow down"*), the evidence, and the open apps using the most of that resource (for example *"Using the most memory: Browser (6.2 GiB), Editor (2.1 GiB)"*). Clicking it opens what to do. Disk space notifies only when critical. Recovery is never announced.
- **Optimize, safely.** For memory, processor or heat issues, Optimize measures your open apps (including the helpers they started) and offers the heaviest ones not in front. After you confirm, it asks them to quit like ⌘Q, so they can ask you to save first. Nothing is force-quit, deleted or changed. Finder, the Dock and Hostwatch itself are never offered.
- **Open at login**, off by default (Settings › Open at login).
- **Matches what macOS shows.** Memory figures follow Activity Monitor; disk space follows Storage settings (1 GB = 10⁹ bytes, including space macOS can free on its own).
- **Chinese and English UI**, following the system language with a manual switch.

## Resource use

Measured on an M3 Max, macOS 27.2, 5-second sampling, popover closed: about 14.5 MB physical memory footprint and about 0.13% of one core. A copy left running for 4.7 days stayed flat at 43 MB and 0.14%. Details and caveats are in [docs/validation.md](docs/validation.md).

## Install

Download `Hostwatch-<version>-macos-arm64.zip` from [Releases](https://github.com/corn12138/hostwatch/releases) (Apple silicon, macOS 13 or later), unzip it, and drag `Hostwatch.app` to Applications.

The app is ad-hoc signed and **not notarized**, so macOS blocks the first launch. Choose *Open Anyway* in System Settings › Privacy & Security, or run:

```sh
xattr -dr com.apple.quarantine /Applications/Hostwatch.app
```

On first launch macOS asks whether Hostwatch may send notifications. If you decline, the menu bar keeps working, and you can change your mind in Settings › Notifications.

## Command line

The same binary is also a CLI:

```sh
hostwatch snapshot --processes          # one reading, with the top processes
hostwatch watch --interval 10 --json    # continuous readings and alert events as JSON
hostwatch menubar --interval 30         # the menu-bar app, from a terminal
```

JSON output uses raw bytes and bytes/s, `schema_version: 2`, and reports each data source's status. Notifications and open at login need Hostwatch.app; they are unavailable when `menubar` runs from a terminal.

## Privacy and limits

- Hostwatch reads system statistics only. It uploads nothing, stores no history on disk, and never reads file contents.
- It changes system state only when you confirm a quit request in Optimize, or turn on Open at login.
- Thresholds are Hostwatch's own heuristics, not Apple's: swap traffic ≥ 1 MiB/s (critical ≥ 16 MiB/s), processor ≥ 90%, free disk space under 20 GB or 15% (critical under 5 GB or 5%), and the macOS cooling level Hot (critical: Critical). Temperatures alone never alert.
- Not yet: disk I/O, folder size scans, which app started rising before an alert, notarized builds, Intel builds, and Windows (the Windows entry point says so instead of showing fake data).

## Build

```sh
./scripts/cargo.sh test --locked
./scripts/cargo.sh clippy --locked --all-targets -- -D warnings
./scripts/package-app.sh      # dist/Hostwatch.app and a zip
./scripts/install.sh          # build, replace /Applications/Hostwatch.app, reopen
```

`scripts/cargo.sh` works around Cargo's trouble with build directories whose path contains `:`; in a normal checkout, plain `cargo` works too. Architecture notes are in [docs/architecture.md](docs/architecture.md) and the roadmap is in [docs/roadmap.md](docs/roadmap.md) (both in Chinese).

## License

[MIT](LICENSE)
