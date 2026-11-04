//! On-demand, read-only folder sizes for a disk-space issue: which folders under a chosen folder
//! take the most room.
//!
//! The scan only reads metadata (`lstat`): it never opens, moves or deletes files, never follows
//! symbolic links and never crosses onto another volume. Sizes are space allocated on disk, and a
//! file with several hard links is counted once. Folders macOS guards with a permission prompt are
//! listed but not entered unless the person asks for that folder. Folders are measured in turns
//! of `CHUNK` items each, so when the scan stops at `BUDGET` (or is cancelled) every folder has a
//! partial size, reported as a lower bound, instead of a few finished and the rest unseen.

use std::collections::HashSet;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub const BUDGET: Duration = Duration::from_secs(20);
/// Entries between checks of the clock and the cancel flag.
const CHECK_EVERY: u64 = 2048;
/// Entries one folder measures before the next folder gets a turn.
const CHUNK: u64 = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Folder,
    File,
    /// macOS asks permission before an app reads it; measured only when the person picks it.
    Protected,
    /// The folder itself could not be listed.
    Unreadable,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub kind: Kind,
    /// Part of it was unreadable, guarded or not reached: the size is at least `bytes`.
    pub partial: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    Budget,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub root: PathBuf,
    /// Largest first.
    pub entries: Vec<Entry>,
    pub total: u64,
    /// Files and folders looked at.
    pub items: u64,
    /// Folders inside that could not be listed (usually other apps' or system data).
    pub unreadable: u64,
    /// Guarded folders inside the entries that were not entered.
    pub guarded: u64,
    /// Folders whose measurement the stop cut short.
    pub unfinished: usize,
    pub stopped: Option<Stop>,
}

struct Walk<'a> {
    device: u64,
    guarded: &'a [PathBuf],
    seen: HashSet<(u64, u64)>,
    deadline: Instant,
    cancel: &'a AtomicBool,
    progress: &'a AtomicU64,
    items: u64,
    unreadable: u64,
    guarded_inside: u64,
    stopped: Option<Stop>,
}

impl Walk<'_> {
    /// Allocated bytes, counting each hard-linked file once.
    fn size(&mut self, meta: &std::fs::Metadata) -> u64 {
        if !meta.is_dir() && meta.nlink() > 1 && !self.seen.insert((meta.dev(), meta.ino())) {
            return 0;
        }
        meta.blocks() * 512
    }

    fn tick(&mut self) -> bool {
        self.items += 1;
        if self.items % CHECK_EVERY == 0 {
            self.progress.store(self.items, Ordering::Relaxed);
            if self.cancel.load(Ordering::Relaxed) {
                self.stopped = Some(Stop::Cancelled);
            } else if Instant::now() >= self.deadline {
                self.stopped = Some(Stop::Budget);
            }
        }
        self.stopped.is_none()
    }

    /// Measures one folder's turn: whole directory listings until about `CHUNK` items are done.
    fn turn(&mut self, folder: &mut Pending) {
        let until = self.items + CHUNK;
        while self.items < until && self.stopped.is_none() {
            let Some(current) = folder.stack.pop() else {
                return;
            };
            let Ok(listing) = std::fs::read_dir(&current) else {
                self.unreadable += 1;
                folder.partial = true;
                continue;
            };
            for item in listing {
                if !self.tick() {
                    // The rest of this listing is not counted.
                    folder.cut = true;
                    return;
                }
                // `DirEntry::metadata` does not follow symbolic links.
                let Ok(item) = item else {
                    folder.partial = true;
                    continue;
                };
                let Ok(meta) = item.metadata() else {
                    folder.partial = true;
                    continue;
                };
                folder.bytes += self.size(&meta);
                if meta.is_dir() {
                    let path = item.path();
                    if meta.dev() != self.device {
                        continue;
                    }
                    if self.guarded.contains(&path) {
                        self.guarded_inside += 1;
                        folder.partial = true;
                        continue;
                    }
                    folder.stack.push(path);
                }
            }
        }
    }
}

/// A top-level folder still being measured.
struct Pending {
    stack: Vec<PathBuf>,
    bytes: u64,
    partial: bool,
    /// The stop came in the middle of one of its listings.
    cut: bool,
}

/// Sizes of everything directly inside `root`. `guarded` folders are listed without being entered,
/// except `root` itself when the person chose it.
pub fn scan(
    root: &Path,
    guarded: &[PathBuf],
    budget: Duration,
    cancel: &AtomicBool,
    progress: &AtomicU64,
) -> Result<Report, String> {
    let meta = std::fs::symlink_metadata(root).map_err(|e| e.to_string())?;
    let listing = std::fs::read_dir(root).map_err(|e| e.to_string())?;
    let mut walk = Walk {
        device: meta.dev(),
        guarded,
        seen: HashSet::new(),
        deadline: Instant::now() + budget,
        cancel,
        progress,
        items: 0,
        unreadable: 0,
        guarded_inside: 0,
        stopped: None,
    };
    let mut children: Vec<(PathBuf, std::fs::Metadata)> = listing
        .filter_map(|item| {
            let item = item.ok()?;
            Some((item.path(), item.metadata().ok()?))
        })
        .collect();
    children.sort_by(|a, b| a.0.cmp(&b.0));
    let mut entries = Vec::new();
    let mut pending: Vec<(usize, Pending)> = Vec::new();
    for (path, meta) in children {
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let own = walk.size(&meta);
        let (bytes, kind, partial) = if !meta.is_dir() {
            (own, Kind::File, false)
        } else if meta.dev() != walk.device {
            // Another volume mounted here: not part of this disk.
            continue;
        } else if guarded.contains(&path) {
            (0, Kind::Protected, true)
        } else if std::fs::read_dir(&path).is_err() {
            walk.unreadable += 1;
            (own, Kind::Unreadable, true)
        } else {
            pending.push((
                entries.len(),
                Pending {
                    stack: vec![path.clone()],
                    bytes: 0,
                    partial: false,
                    cut: false,
                },
            ));
            (own, Kind::Folder, false)
        };
        entries.push(Entry {
            name,
            path,
            bytes,
            kind,
            partial,
        });
    }
    // Round-robin until every folder is done or the scan stops.
    while walk.stopped.is_none() && pending.iter().any(|(_, p)| !p.stack.is_empty()) {
        for (_, folder) in pending.iter_mut().filter(|(_, p)| !p.stack.is_empty()) {
            walk.turn(folder);
            if walk.stopped.is_some() {
                break;
            }
        }
    }
    let mut unfinished = 0;
    for (i, folder) in pending {
        let unfinished_here = folder.cut || !folder.stack.is_empty();
        unfinished += usize::from(unfinished_here);
        let entry = &mut entries[i];
        entry.bytes += folder.bytes;
        entry.partial = folder.partial || unfinished_here;
    }
    entries.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.name.cmp(&b.name)));
    walk.progress.store(walk.items, Ordering::Relaxed);
    Ok(Report {
        root: root.to_path_buf(),
        total: entries.iter().map(|e| e.bytes).sum(),
        entries,
        items: walk.items,
        unreadable: walk.unreadable,
        guarded: walk.guarded_inside,
        unfinished,
        stopped: walk.stopped,
    })
}

/// Folders under `home` that make macOS ask the person before an app reads them.
pub fn guarded(home: &Path) -> Vec<PathBuf> {
    [
        "Desktop",
        "Documents",
        "Downloads",
        "Library/Mobile Documents",
        "Library/Containers",
        "Library/Group Containers",
    ]
    .iter()
    .map(|p| home.join(p))
    .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub enum SpaceState {
    Scanning {
        root: PathBuf,
        since: u64,
        items: u64,
    },
    Done {
        at: u64,
        report: Report,
    },
    Failed {
        root: PathBuf,
        at: u64,
        error: String,
    },
}

/// The space page's state; one scan at a time.
pub struct Space {
    home: PathBuf,
    state: Option<SpaceState>,
}

impl Space {
    pub fn new(home: PathBuf) -> Self {
        Self { home, state: None }
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn state(&self) -> Option<&SpaceState> {
        self.state.as_ref()
    }

    pub fn scanning(&self) -> bool {
        matches!(self.state, Some(SpaceState::Scanning { .. }))
    }

    /// Starts a scan of a folder inside home; refuses to overlap one in progress or to leave home.
    pub fn begin(&mut self, root: PathBuf, now: u64) -> bool {
        if self.scanning() || !root.starts_with(&self.home) {
            return false;
        }
        self.state = Some(SpaceState::Scanning {
            root,
            since: now,
            items: 0,
        });
        true
    }

    pub fn progress(&mut self, count: u64) {
        if let Some(SpaceState::Scanning { items, .. }) = &mut self.state {
            *items = count;
        }
    }

    pub fn finish(&mut self, result: Result<Report, String>, now: u64) {
        let Some(SpaceState::Scanning { root, .. }) = self.state.take() else {
            return;
        };
        self.state = Some(match result {
            Ok(report) => SpaceState::Done { at: now, report },
            Err(error) => SpaceState::Failed {
                root,
                at: now,
                error,
            },
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Dir(PathBuf);

    impl Dir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "hostwatch-space-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Dir(path)
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![7u8; bytes]).unwrap();
    }

    fn run(root: &Path, guarded: &[PathBuf]) -> Report {
        scan(
            root,
            guarded,
            BUDGET,
            &AtomicBool::new(false),
            &AtomicU64::new(0),
        )
        .unwrap()
    }

    #[test]
    fn sizes_folders_without_following_links_or_counting_hard_links_twice() {
        let dir = Dir::new("sizes");
        let root = &dir.0;
        write(&root.join("big/a/one.bin"), 2_000_000);
        write(&root.join("big/two.bin"), 1_000_000);
        write(&root.join("small/one.bin"), 10_000);
        write(&root.join("loose.bin"), 500_000);
        // A second name for the same file adds nothing.
        fs::hard_link(root.join("big/two.bin"), root.join("big/two-again.bin")).unwrap();
        // A link to the big folder is not followed.
        std::os::unix::fs::symlink(root.join("big"), root.join("small/link")).unwrap();
        let report = run(root, &[]);
        let names: Vec<&str> = report.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["big", "loose.bin", "small"]);
        let big = &report.entries[0];
        assert_eq!(big.kind, Kind::Folder);
        assert!(
            big.bytes >= 3_000_000 && big.bytes < 3_200_000,
            "{}",
            big.bytes
        );
        assert!(!big.partial);
        assert_eq!(report.entries[1].kind, Kind::File);
        assert!(
            report.entries[2].bytes < 100_000,
            "the link was not followed"
        );
        assert_eq!(
            report.total,
            report.entries.iter().map(|e| e.bytes).sum::<u64>()
        );
        assert!(report.stopped.is_none());
    }

    #[test]
    fn guarded_folders_are_listed_but_not_entered_unless_chosen() {
        let dir = Dir::new("guarded");
        let root = &dir.0;
        write(&root.join("Documents/report.bin"), 1_000_000);
        write(&root.join("Library/Containers/app/data.bin"), 1_000_000);
        write(&root.join("Library/Caches/cache.bin"), 200_000);
        let guarded = guarded(root);
        let report = run(root, &guarded);
        let documents = report
            .entries
            .iter()
            .find(|e| e.name == "Documents")
            .unwrap();
        assert_eq!((documents.kind, documents.bytes), (Kind::Protected, 0));
        let library = report.entries.iter().find(|e| e.name == "Library").unwrap();
        assert!(library.partial);
        assert!(library.bytes < 500_000, "containers were not entered");
        assert_eq!(report.guarded, 1);
        // Chosen by the person: the guarded folder itself is scanned.
        let chosen = run(&root.join("Documents"), &guarded);
        assert_eq!(chosen.entries[0].name, "report.bin");
    }

    #[test]
    fn cancelled_and_over_budget_scans_report_what_they_measured() {
        let dir = Dir::new("stop");
        let root = &dir.0;
        // Three folders of many small subfolders: 3 × 40 × 20 entries, past the first check.
        for i in 0..3 {
            for j in 0..40 {
                for k in 0..20 {
                    write(&root.join(format!("f{i}/{j}/{k}.bin")), 1);
                }
            }
        }
        let cancelled = scan(
            root,
            &[],
            BUDGET,
            &AtomicBool::new(true),
            &AtomicU64::new(0),
        )
        .unwrap();
        assert_eq!(cancelled.stopped, Some(Stop::Cancelled));
        // Taking turns, every folder was measured a little before the stop; none finished.
        assert_eq!(cancelled.unfinished, 3);
        assert!(cancelled.entries.iter().all(|e| e.partial && e.bytes > 0));
        let progress = AtomicU64::new(0);
        let late = scan(
            root,
            &[],
            Duration::ZERO,
            &AtomicBool::new(false),
            &progress,
        )
        .unwrap();
        assert_eq!(late.stopped, Some(Stop::Budget));
        assert_eq!(progress.load(Ordering::Relaxed), late.items);
        assert!(scan(
            &root.join("missing"),
            &[],
            BUDGET,
            &AtomicBool::new(false),
            &progress
        )
        .is_err());
    }

    #[test]
    fn one_scan_at_a_time_and_only_inside_home() {
        let mut space = Space::new(PathBuf::from("/Users/me"));
        assert!(!space.begin(PathBuf::from("/System"), 0));
        assert!(space.begin(PathBuf::from("/Users/me/Library"), 0));
        assert!(!space.begin(PathBuf::from("/Users/me"), 1), "no overlap");
        space.progress(42);
        assert!(matches!(
            space.state(),
            Some(SpaceState::Scanning { items: 42, .. })
        ));
        space.finish(Err("denied".into()), 2);
        assert!(matches!(space.state(), Some(SpaceState::Failed { .. })));
        assert!(space.begin(PathBuf::from("/Users/me"), 3));
    }
}

/// Manual check against the real home folder (reads metadata only, skips guarded folders):
/// `cargo test real_home -- --ignored --nocapture`.
#[cfg(test)]
#[test]
#[ignore]
fn real_home() {
    let home = PathBuf::from(std::env::var("HOME").unwrap());
    let started = Instant::now();
    let report = scan(
        &home,
        &guarded(&home),
        BUDGET,
        &AtomicBool::new(false),
        &AtomicU64::new(0),
    )
    .unwrap();
    println!(
        "{:?} items={} unreadable={} guarded={} unfinished={} stopped={:?}",
        started.elapsed(),
        report.items,
        report.unreadable,
        report.guarded,
        report.unfinished,
        report.stopped
    );
    for e in report.entries.iter().take(8) {
        println!(
            "{:>8.1} GB {:?} partial={} {}",
            e.bytes as f64 / 1e9,
            e.kind,
            e.partial,
            e.name
        );
    }
}
