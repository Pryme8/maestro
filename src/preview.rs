//! What an agent is making: the pictures written in its folder while it
//! works, found and decoded off the window's thread so they can be shown as
//! they change.
//!
//! Nothing here runs unless the preview is switched on, and switching it off
//! stops the thread and frees the picture. Only picture files are opened,
//! only inside the session's folder, and links out of that folder are not
//! followed.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};

/// The kinds of file that can be shown.
pub const KINDS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp"];

/// Folders that hold other people's files, never the agent's work.
const SKIP: &[&str] = &["node_modules", "target", "venv", "__pycache__"];

/// How many folder entries one look may visit before it gives up.
const VISIT_LIMIT: usize = 20_000;
const DEPTH_LIMIT: usize = 8;
/// A picture file larger than this is listed and not opened.
pub const FILE_LIMIT: u64 = 64 * 1024 * 1024;
/// The memory a decoder may ask for. A small file can claim to be enormous.
const DECODE_LIMIT: u64 = 256 * 1024 * 1024;
/// The long side of the picture kept for showing, so one preview costs at
/// most about 6 MB however large the file is.
pub const LONG_SIDE: u32 = 1280;
/// How many pictures are listed.
pub const LISTED: usize = 8;
/// How often the folder is looked at again.
const EVERY: Duration = Duration::from_millis(1500);

/// A picture file found in the folder.
#[derive(Clone, Debug, PartialEq)]
pub struct Made {
    pub path: PathBuf,
    /// The path below the folder, with forward slashes.
    pub name: String,
    pub changed_ms: u64,
    pub bytes: u64,
}

#[derive(Default, Debug)]
pub struct Found {
    /// Newest first.
    pub made: Vec<Made>,
    /// The folder was too large to look through in full.
    pub cut_short: bool,
}

fn is_picture(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| KINDS.iter().any(|k| e.eq_ignore_ascii_case(k)))
}

/// The pictures in `folder` changed at or after `since_ms`, newest first.
pub fn find(folder: &Path, since_ms: u64, keep: usize) -> Found {
    let mut found = Found::default();
    let mut visited = 0usize;
    let mut stack = vec![(folder.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            visited += 1;
            if visited > VISIT_LIMIT {
                found.cut_short = true;
                stack.clear();
                break;
            }
            // The entry's own kind: a link is reported as a link and left alone.
            let Ok(kind) = entry.file_type() else { continue };
            let path = entry.path();
            if kind.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') || SKIP.contains(&name.as_str()) {
                    continue;
                }
                if depth + 1 > DEPTH_LIMIT {
                    found.cut_short = true;
                    continue;
                }
                stack.push((path, depth + 1));
            } else if kind.is_file() && is_picture(&path) {
                let Ok(meta) = entry.metadata() else { continue };
                let changed_ms = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_millis() as u64);
                if changed_ms < since_ms {
                    continue;
                }
                let name = path.strip_prefix(folder).unwrap_or(&path).to_string_lossy().replace('\\', "/");
                found.made.push(Made { path, name, changed_ms, bytes: meta.len() });
            }
        }
    }
    found.made.sort_by(|a, b| b.changed_ms.cmp(&a.changed_ms).then_with(|| a.name.cmp(&b.name)));
    found.made.truncate(keep);
    found
}

/// A decoded picture, scaled down for showing.
pub struct Picture {
    pub width: usize,
    pub height: usize,
    /// Four bytes a pixel: red, green, blue, opacity.
    pub rgba: Vec<u8>,
    /// The size of the file's own picture.
    pub full: (u32, u32),
}

/// Decode a picture file. An animation gives its first frame.
pub fn load(path: &Path, long_side: u32) -> Result<Picture, String> {
    let bytes = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if bytes > FILE_LIMIT {
        return Err(format!("The file is {} MB, more than is opened for a preview.", bytes / (1024 * 1024)));
    }
    let mut reader = image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(DECODE_LIMIT);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|e| e.to_string())?;
    let full = (decoded.width(), decoded.height());
    let shown = if full.0.max(full.1) > long_side { decoded.thumbnail(long_side, long_side) } else { decoded };
    let rgba = shown.to_rgba8();
    Ok(Picture { width: rgba.width() as usize, height: rgba.height() as usize, rgba: rgba.into_raw(), full })
}

/// What the watching thread has for the window.
#[derive(Default)]
pub struct State {
    pub made: Vec<Made>,
    pub cut_short: bool,
    /// The picture to show. `None` follows the newest.
    pub pick: Option<PathBuf>,
    /// A picture decoded since the window last took one.
    pub fresh: Option<(Made, Picture)>,
    /// Why the chosen picture could not be shown.
    pub problem: Option<String>,
    /// The folder has been looked at once.
    pub looked: bool,
}

/// A thread that keeps looking at one folder until it is dropped.
pub struct Watch {
    pub state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    thread: std::thread::Thread,
}

impl Watch {
    pub fn start(folder: PathBuf, since_ms: u64, wake: impl Fn() + Send + 'static) -> Self {
        let state: Arc<Mutex<State>> = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let (shared, stopping) = (state.clone(), stop.clone());
        let handle = std::thread::spawn(move || {
            // The file last decoded, so an unchanged one is not decoded again.
            let mut shown: Option<Made> = None;
            while !stopping.load(Ordering::Relaxed) {
                let found = find(&folder, since_ms, LISTED);
                let pick = shared.lock().ok().and_then(|s| s.pick.clone());
                let want = match &pick {
                    Some(path) => found.made.iter().find(|m| &m.path == path),
                    None => None,
                }
                .or(found.made.first())
                .cloned();
                let mut fresh = None;
                let mut problem = None;
                if let Some(want) = want.filter(|w| shown.as_ref() != Some(w)) {
                    match load(&want.path, LONG_SIDE) {
                        Ok(picture) => {
                            shown = Some(want.clone());
                            fresh = Some((want, picture));
                        }
                        // A file still being written fails here and is tried again next time.
                        Err(err) => problem = Some(format!("{}: {err}", want.name)),
                    }
                }
                if let Ok(mut state) = shared.lock() {
                    let changed = fresh.is_some()
                        || !state.looked
                        || state.made != found.made
                        || state.problem != problem;
                    state.made = found.made;
                    state.cut_short = found.cut_short;
                    state.problem = problem;
                    state.looked = true;
                    if fresh.is_some() {
                        state.fresh = fresh;
                    }
                    if changed {
                        wake();
                    }
                }
                std::thread::park_timeout(EVERY);
            }
        });
        Self { state, stop, thread: handle.thread().clone() }
    }

    /// Choose the picture to show, or `None` to follow the newest, and look again now.
    pub fn pick(&self, path: Option<PathBuf>) {
        if let Ok(mut state) = self.state.lock() {
            state.pick = path;
        }
        self.thread.unpark();
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.unpark();
    }
}

/// `--preview <folder> [minutes]` lists the pictures changed there lately.
pub fn print(folder: &str, minutes: u64) {
    let since = crate::live::now_ms().saturating_sub(minutes * 60_000);
    let found = find(Path::new(folder), since, LISTED);
    if found.made.is_empty() {
        println!("No picture has changed in {folder} in the last {minutes} minutes.");
    }
    for made in &found.made {
        let what = match load(&made.path, LONG_SIDE) {
            Ok(p) => format!("{} by {}, shown at {} by {}", p.full.0, p.full.1, p.width, p.height),
            Err(err) => format!("cannot be shown: {err}"),
        };
        println!("{}  ·  {} bytes  ·  {what}", made.name, made.bytes);
    }
    if found.cut_short {
        println!("The folder is too large to look through in full.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("maestro-preview-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn picture(path: &Path, w: u32, h: u32) {
        let pixels: Vec<u8> = (0..w * h * 3).map(|i| (i % 251) as u8).collect();
        image::save_buffer(path, &pixels, w, h, image::ColorType::Rgb8).unwrap();
    }

    #[test]
    fn every_listed_kind_of_picture_can_be_read_back() {
        let dir = folder("kinds");
        for kind in KINDS {
            let path = dir.join(format!("shot.{kind}"));
            picture(&path, 40, 20);
            let shown = load(&path, LONG_SIDE).unwrap_or_else(|e| panic!("{kind}: {e}"));
            assert_eq!((shown.full, shown.width, shown.height), ((40, 20), 40, 20), "{kind}");
            assert_eq!(shown.rgba.len(), 40 * 20 * 4, "{kind}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_large_picture_is_kept_small() {
        let dir = folder("large");
        let path = dir.join("wide.png");
        picture(&path, 3000, 600);
        let shown = load(&path, LONG_SIDE).unwrap();
        assert_eq!(shown.full, (3000, 600));
        assert_eq!((shown.width, shown.height), (1280, 256));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_pictures_in_the_agents_own_folders_are_found() {
        let dir = folder("find");
        for sub in ["out/deep", ".git", "node_modules/pkg", "target"] {
            std::fs::create_dir_all(dir.join(sub)).unwrap();
        }
        picture(&dir.join("top.png"), 4, 4);
        picture(&dir.join("out/deep/chart.JPG"), 4, 4);
        picture(&dir.join(".git/hidden.png"), 4, 4);
        picture(&dir.join("node_modules/pkg/logo.png"), 4, 4);
        picture(&dir.join("target/built.png"), 4, 4);
        std::fs::write(dir.join("notes.txt"), "not a picture").unwrap();

        let mut names: Vec<String> = find(&dir, 0, LISTED).made.into_iter().map(|m| m.name).collect();
        names.sort();
        assert_eq!(names, ["out/deep/chart.JPG", "top.png"]);

        // Nothing is older than a time in the future, and the list is capped.
        assert!(find(&dir, crate::live::now_ms() + 60_000, LISTED).made.is_empty());
        assert_eq!(find(&dir, 0, 1).made.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_is_not_a_picture_is_refused_and_not_a_crash() {
        let dir = folder("broken");
        let path = dir.join("half.png");
        std::fs::write(&path, b"\x89PNG\r\n\x1a\nnot finished").unwrap();
        assert!(load(&path, LONG_SIDE).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_watch_shows_the_newest_and_then_the_one_picked() {
        let dir = folder("watch");
        picture(&dir.join("a.png"), 8, 8);
        let watch = Watch::start(dir.clone(), 0, || {});
        let wait = |want: &str| {
            for _ in 0..200 {
                if let Some((made, _)) = watch.state.lock().unwrap().fresh.take() {
                    if made.name == want {
                        return true;
                    }
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            false
        };
        assert!(wait("a.png"));
        std::thread::sleep(Duration::from_millis(30));
        picture(&dir.join("b.png"), 8, 8);
        watch.pick(None);
        assert!(wait("b.png"), "a newer picture replaces the one showing");
        watch.pick(Some(dir.join("a.png")));
        assert!(wait("a.png"), "a picked picture is shown instead of the newest");
        drop(watch);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
