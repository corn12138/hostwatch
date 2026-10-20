# Hostwatch: implement the approved minimal native popover

Date: 2026-10-19

## Task and authority

Implement the complete scoped native macOS interface using this selected design:

https://claude.ai/artifact/Y6pBFi1m5du5nR5jre8HQS#page-9539465e2f90

Repository: `/Users/huangyuming/Desktop/monitor-Mac:Windows`.

Read this task, `docs/design-review-final-2026-10-19/review.md`, and the current architecture, roadmap, validation notes and source before changing code. The current review includes seven locally captured prototype screenshots. Use those as supporting references if artifact access is unavailable; disclose any inaccessible design details instead of inventing them.

The visual direction is already selected. Proceed through implementation and verification, not another design exploration, a plan-only response, a home-screen-only implementation, or another web prototype. Make routine implementation decisions autonomously within scope. Complete unblocked work and distinguish real blockers from unfinished work. This task authorizes implementation, not commits, pushes, publishing, permission changes or system installation.

The user's highest priority is **minimalism, clarity, efficiency and practicality for people who do not understand computers**. In a few seconds, users should understand whether anything needs attention and what to do first. Complete user journeys without expanding homepage complexity.

## Current baseline: preserve it

Inspect `git status` and the actual working tree first. Existing modified/untracked files are user/Claude work, not disposable scaffolding. Do not reset, clean, overwrite or commit unrelated changes. Do not modify `docs/juejin/` or unrelated handover notes.

The repository already has a Rust macOS collector, advice rules, alert Engine, bounded History, CLI and native AppKit menu-bar shell. Relevant files include `src/menu.rs`, `src/menubar.rs`, `src/alerts.rs`, `src/advice.rs`, `src/model.rs`, `src/platform/macos.rs`, `src/cli.rs`, `src/main.rs` and `tests/cli.rs`.

The currently inspected source includes common-mode run-loop sampling, change-only status-title updates, partial-data handling for unmeasured fields, and Activity Monitor / Storage Settings launch feedback. Preserve these behaviors. Earlier audit reports may describe already-fixed issues; reconcile them with current code before reporting or changing anything.

Keep Rust 1.79 compatibility and the existing objc2/AppKit direction. Use `./scripts/cargo.sh` for every Cargo invocation because direct Cargo builds from this colon-containing checkout path are unsafe. Preserve CLI output, JSON schema, exit codes, flag validation and supported interval range.

## Product surfaces: all required this round

Use one compact native popover anchored to the status item, with shallow in-panel navigation and clear Back behavior. Follow the design's approximately 440-point width, restrained spacing, native colors/materials and readable Chinese typography. Adjust details where necessary for real native readability and accessibility. Normal home content should fit without scrolling; allow appropriate scrolling for longer detail content on smaller screens.

1. **Status item and home:** one plain-language conclusion, three concise resource rows (memory, processor, disk space), only useful small trends, one highest-priority recommendation when needed, sample freshness, pause/resume and More. Use “目前无需处理” or a similarly bounded conclusion. Open/close via the status item, outside click and Escape. Reopening must show current state, not a stale frozen view.
2. **Resource details:** implement the shared template for all three resources. Memory shows actual pressure, usage/components and supported trends; processor shows actual usage/history; disk shows startup-volume available capacity and an honest capacity display. Provide short explanations, insufficient/missing-history states and return navigation. No directory scan or disk-I/O claims.
3. **Recommendations and handling:** support memory pressure, swap activity, sustained CPU load and low disk space. Use one reason, a few concrete manual steps and a relevant system-tool action. Implement the additional-issues list. Group related memory/swap presentation without losing highest severity, individual rule evidence or recovery requirements. Make counts accurately describe grouped issues versus underlying rules.
4. **On-demand processes:** show an initial not-yet-read state, explicit refresh, loading, actual readable RSS top five, sample time, empty/failure feedback and any genuinely detectable access limitation. Explain briefly that large RSS does not prove causation. Keep this auxiliary and on demand.
5. **Settings and More:** preserve interval selection, pause/resume, concise metric/source explanations and quit. Retain all existing presets (5, 10, 30, 60, 300 seconds) and CLI custom intervals from 5 through 3600 seconds; represent an active custom value correctly. Do not reduce support to the prototype's two demo options. No nonfunctional notification/login-item switches.
6. **Feedback and boundary states:** loading, rate warmup, normal, warning, critical, multiple issues, partial availability, full failure, stale data, pause, recovery observation, confirmed recovery, tool launch failure and retry. Reuse components and state variants instead of creating unnecessary pages.

Every visible control must work and have a meaningful result or truthful failure. Do not ship dead controls, placeholders, prototype scene switches, fabricated readings, simulated tool launches, or the design tool's outer control panel.

## Mandatory corrections to the design reference

- **One authoritative data source:** current values, chart endpoints, tooltips, textual explanations and accessibility summaries must agree. The prototype contains numeric inconsistencies (for example CPU 18% versus a chart's latest 26%); do not reproduce them.
- **Time-correct history:** retain the bounded 120-sample capacity. Plot actual sample timestamps and label the actual available time span. Never calculate existing history from `120 × current interval`, redistribute old samples after interval changes, fabricate missing points, connect lines across unavailable periods or fill gaps with zero. Use recorded evidence for gap explanations; a large timestamp interval alone does not prove sleep. Preserve numeric absence as absence.
- **Honest chart scales:** keep percentages interpretable and units explicit in details. Combined swap read+write traffic drives the existing swap rule. A 0–30 MiB/s demonstration axis must not clip higher real values; accommodate actual data and show relevant thresholds only where useful. Do not invent directional swap history, pressure history or 24-hour history that the collector does not retain.
- **Correct memory semantics:** current macOS `used_bytes` equals app + wired + compressed, with cached files tracked separately. Correct prototype copy implying that used memory includes cache. Do not change the collector definition to match the mockup. High used RAM or historical swap allocation alone is not a cleanup recommendation.
- **Engine-owned recovery:** monitoring is already active without opening a system tool. Clicking a handling button must not start, reset, advance or finish recovery. Read existing policy/state: warning requires 2 abnormal samples, critical 1, recovery 2 valid normal samples, cooldown 600 seconds. Unknown observations do not advance or clear streaks. Keep these semantics unchanged. Add narrowly scoped read-only state access if needed; do not create a parallel UI alert engine.
- **Current versus triggering evidence:** retained alert evidence is historical when the current reading changes or becomes unavailable. Label its trigger time and distinguish it from current data. Do not present the old critical value as a current reading while recovering. Grouped recommendations resolve only when all relevant active rules have actually recovered.
- **Failure is not health:** missing, warmup, stale, paused and failed states must remain distinguishable. Do not show stale values as current or clear active alerts after collection failure. Avoid claiming the whole computer is healthy when only supported resources were checked. Source counts and availability come from actual sources, not demo constants.
- **Truthful actions and timing:** a tool-launch acceptance is not proof that a particular tab is visible or that the problem is fixed. Give a short manual fallback when launch fails. Fewer than five process results does not establish permission denial. Calculate observed duration from timestamps, not streak count multiplied by the current interval. Avoid promised speedups or precise releasable-memory estimates.

## Architecture and resource constraints

Use a single native macOS process, the existing Rust collector/advice/Engine/History and AppKit status item with a lightweight native popover and native chart drawing. Do not introduce WebView, Electron, Tauri, a frontend runtime, helper processes, cloud AI, network telemetry or uploads. The HTML artifact is a reference, not the production architecture.

Keep presentation/state transformations testable without AppKit. Small read-only core accessors and bounded UI metadata are acceptable where necessary; avoid broad core refactors or public CLI serialization changes. Prefer shared components and clear module boundaries over a monolithic view or generic UI framework.

AppKit work belongs on the main thread. Keep sampling and on-demand process reads responsive, avoid overlapping scans and stale asynchronous results, and use safe ownership/lifetime handling for timers, callbacks and views. Any added worker must stop cleanly at quit. Avoid perpetual animation and redundant view/title reconstruction.

Closing the popover stops invisible rendering, not monitoring. Sampling must continue while the popover or More menu is open. Explicit pause stops the sampling timer; changing interval while paused must not restart it. Resume establishes a fresh baseline before CPU/swap rates are treated as measured and uses the selected interval. Keep existing active alerts/cooldown semantics. Handle actual wake or elapsed sampling gaps without pretending rates are current across an invalid baseline.

Use native light/dark semantic colors, meaningful text/symbols in addition to color, readable secondary text, keyboard focus, accessible labels, Escape and Back behavior. Do not move necessary status information into hover-only content. Verify at native size, not only in a scaled browser canvas.

No automatic cleanup, process killing, filesystem mutation, permission escalation, directory scanning, login item, notification feature, Windows GUI, signing/notarization or release packaging in this scope. A minimal local development app bundle is acceptable only if needed to run/inspect the native interface; do not install or register it at login.

Keep comments limited to non-obvious reasons and constraints in the final implementation. Do not preserve failed approaches or development history in comments or PR-style descriptions.

## Verification and acceptance

Run the existing required checks using the repository wrapper, including formatting, clippy with warnings denied, unit/integration tests and a release build. Use the repository's documented exact flags; typical commands are:

```sh
./scripts/cargo.sh fmt -- --check
./scripts/cargo.sh clippy --locked --all-targets -- -D warnings
./scripts/cargo.sh test --locked
./scripts/cargo.sh build --release --locked
```

Add focused regression tests for meaningful behavior introduced by this change: values/chart agreement; variable-interval timestamp history and gaps; missing data versus zero; retained alert evidence versus current readings; grouped severity and recovery; failure retaining alerts; pause/interval/resume state; and on-demand result lifecycle where applicable. Preserve adjacent CLI coverage. Do not merely assert implementation constants or duplicate view code in tests.

Exercise the native journeys and record actual results:

1. Launch one menu-bar app with no Dock icon; open, close and reopen. Observe sampling both while closed and while the popover/More menu is open.
2. Visit memory, processor and disk details; inspect charts, point descriptions, missing history and return paths.
3. Open handling instructions for supported rule types; launch the relevant real system tool where possible; verify useful failure feedback without fabricating success.
4. Inspect multiple issues, preserving all severity and recovery state; verify automatic observation without any action click.
5. Open processes, explicitly refresh, observe loading/results and supported error/empty cases; verify normal sampling does not start repeated process scans.
6. Pause, change interval while paused, resume, verify fresh rate baseline and subsequent actual rates. Check interval consistency across the interface.
7. Verify partial/full failure, stale data and recovery using deterministic fixtures for state logic. Test actual sleep/wake where feasible and label simulated versus physical tests separately.
8. Verify light/dark, keyboard/Escape/Back, accessible state descriptions and readability at native size; quit and verify actual exit code 0 and no leftover application process.

Save representative native screenshots and test evidence. If automation or screen/accessibility access is unavailable, do not change permissions or pretend screenshots/native clicks passed. Complete the implementation and automated tests, identify the precise remaining manual checks, and report the limitation.

Measure the final release app rather than inheriting performance claims from the old text menu. Record environment, revision/dirty state, interval, elapsed time, scenario and raw data for RSS, physical footprint and CPU. Compare popover closed, open, after repeated opens and an on-demand process scan. Run a controlled 30-minute session where feasible, including a 5-second-interval case. Repeated-open tests should use the normal application event loop and appropriate autorelease pools; avoid the earlier no-pool harness artifact. A 200-open plateau check is useful if real native automation is available. Do not call an unrun 8-hour or sleep/wake test passed.

The original RSS <=40 MiB target has not been met by the existing implementation. Physical footprint <=40 MB is a proposed alternative, not an approved replacement. Report both measures and retain the pending budget decision. This decision does not block implementing and validating the interface. Do not assert that all AppKit apps necessarily have the same RSS, or that the entire observed RSS is shared memory, without evidence.

## Deliverables

- Working native implementation covering all scoped surfaces, controls and data states, with existing CLI behavior preserved.
- Focused tests, executed check results, native evidence where available, and a clear list of unverified/manual gates.
- Updated relevant architecture/roadmap/validation documentation describing final behavior and measured limits, without rewriting unrelated files or earlier evidence.
- A concise handover: changed files, implemented behavior, tests actually run, native paths actually exercised, performance results and remaining limitations. Do not mark prototype behavior as native proof, change the budget decision, or claim full completion while required controls remain stubs.

Proceed with the complete scoped implementation now. Keep the product simple even when the implementation must handle difficult states correctly.
