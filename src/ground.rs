//! Grounded local rewriting: free, deterministic, and nothing leaves the
//! machine.
//!
//! The end-to-end benchmark showed that LLM rewrites only paid off when they
//! *added information the agent didn't have* (which file a vague request is
//! about), and cost tokens when they added noise (lists of files to read,
//! extra work). So the local rewrite does only two things, and only when the
//! evidence is clear:
//!
//! 1. **Trim pastes**: a pasted log is replaced by its first error block.
//! 2. **Name the likely file**: when one source file clearly matches the
//!    prompt's content words, append a one-line hint.
//!
//! Otherwise the prompt is left unchanged ("the tests are failing" is already
//! an efficient instruction: the agent just runs the tests).

use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::lang::fold;
use crate::lint::{content_words, detect_language};

/// A paragraph this big is treated as a paste.
const PASTE_MIN_LINES: usize = 10;
const PASTE_MIN_BYTES: usize = 1_500;
/// The top file must beat the runner-up by this factor to be named.
const CONFIDENCE_RATIO: f64 = 1.5;
const MAX_FILE_BYTES: u64 = 200_000;
const MAX_FILES: usize = 2_000;
/// Directories never worth describing or scanning.
const SKIP_DIRS: &[&str] = &[
    "target",
    "node_modules",
    "__pycache__",
    "dist",
    "build",
    "venv",
];

#[derive(Debug, Clone, PartialEq)]
pub struct LocalRewrite {
    pub text: String,
    /// False when nothing useful was found: the prompt is returned as-is.
    pub changed: bool,
    pub target: Option<String>,
    /// Bytes removed by trimming pastes.
    pub trimmed_bytes: usize,
}

pub fn rewrite_local(prompt: &str, dir: &Path) -> LocalRewrite {
    let messages = &detect_language(prompt).messages;
    let (mut text, trimmed_bytes) = trim_pastes(prompt, messages.excerpt_note);
    let target = if has_path_anchor(&text) {
        None // the prompt (or its excerpt) already names a file
    } else {
        likely_target(&text, dir)
    };
    if let Some(file) = &target {
        text.push_str("\n\n");
        text.push_str(&messages.likely_location.replace("{file}", file));
    }
    LocalRewrite {
        changed: target.is_some() || trimmed_bytes > 0,
        text,
        target,
        trimmed_bytes,
    }
}

// ---------------------------------------------------------------------------
// Paste trimming
// ---------------------------------------------------------------------------

/// Replace each pasted paragraph that contains an error with that error's
/// block. Returns the new text and the bytes removed.
pub fn trim_pastes(prompt: &str, note: &str) -> (String, usize) {
    let mut removed = 0;
    let paragraphs: Vec<String> = split_paragraphs(prompt)
        .into_iter()
        .map(|para| {
            let lines = para.lines().count();
            if lines < PASTE_MIN_LINES && para.len() < PASTE_MIN_BYTES {
                return para.to_string();
            }
            match first_error_block(para) {
                Some(block) => {
                    let excerpt = format!(
                        "{}\n```\n{block}\n```",
                        note.replace("{n}", &lines.to_string())
                    );
                    removed += para.len().saturating_sub(excerpt.len());
                    excerpt
                }
                None => para.to_string(),
            }
        })
        .collect();
    (paragraphs.join("\n\n"), removed)
}

fn split_paragraphs(text: &str) -> Vec<&str> {
    text.split("\n\n")
        .map(|p| p.trim_matches('\n'))
        .filter(|p| !p.trim().is_empty())
        .collect()
}

/// The first error line and the stack trace that follows it.
fn first_error_block(paste: &str) -> Option<String> {
    let lines: Vec<&str> = paste.lines().collect();
    let start = lines.iter().position(|l| is_error_line(l))?;
    let mut end = start + 1;
    while end < lines.len() && is_continuation(lines[end]) {
        end += 1;
    }
    Some(lines[start..end].join("\n"))
}

fn is_error_line(line: &str) -> bool {
    let t = line.trim_start();
    t.contains(" ERROR ")
        || t.contains(" FATAL ")
        || t.contains("[ERROR]")
        || t.starts_with("ERROR")
        || t.starts_with("Traceback (most recent call last)")
        || t.contains("panicked at")
        || t.starts_with("Exception in thread")
        || t.starts_with("error[")
        || t.starts_with("error:")
}

/// Stack-trace lines: indented frames, Python/Java/Rust trace markers, and the
/// final `SomethingError: message` line.
fn is_continuation(line: &str) -> bool {
    let t = line.trim_start();
    if t.is_empty() {
        return false;
    }
    line.starts_with(char::is_whitespace)
        || t.starts_with("Traceback")
        || t.starts_with("File \"")
        || t.starts_with("at ")
        || t.starts_with("Caused by")
        || t.starts_with("-->")
        || t.starts_with("note:")
        || t.split(':').next().is_some_and(|head| {
            !head.contains(' ') && (head.ends_with("Error") || head.ends_with("Exception"))
        })
}

fn has_path_anchor(text: &str) -> bool {
    text.split_whitespace().any(|t| {
        let t = t.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '.');
        t.contains('/')
            && t.rsplit_once('.')
                .is_some_and(|(_, ext)| (1..=5).contains(&ext.len()))
    })
}

// ---------------------------------------------------------------------------
// Target grounding
// ---------------------------------------------------------------------------

/// The source file that clearly matches the prompt's content words, if any.
pub fn likely_target(prompt: &str, dir: &Path) -> Option<String> {
    let words: Vec<String> = content_words(prompt)
        .into_iter()
        .filter(|w| w.chars().count() >= 3 && !w.chars().all(|c| c.is_ascii_digit()))
        .collect();
    if words.is_empty() {
        return None;
    }
    let files: Vec<(String, String)> = project_files(dir)
        .into_iter()
        .filter(|f| !is_test_file(f))
        .filter_map(|f| {
            let path = dir.join(&f);
            let size = path.metadata().ok()?.len();
            (size <= MAX_FILE_BYTES)
                .then(|| std::fs::read_to_string(&path).ok())
                .flatten()
                .map(|content| (f, fold(&content)))
        })
        .collect();
    let ranked = rank(&words, &files);
    match ranked.as_slice() {
        [(file, top), rest @ ..] if *top > 0.0 => {
            let second = rest.first().map_or(0.0, |(_, s)| *s);
            (*top >= second * CONFIDENCE_RATIO).then(|| file.clone())
        }
        _ => None,
    }
}

/// Score files by IDF-weighted word matches. Path matches beat content
/// matches; adjacent words that appear as one identifier score highest.
fn rank(words: &[String], files: &[(String, String)]) -> Vec<(String, f64)> {
    let stems: Vec<String> = words.iter().map(|w| stem(w)).collect();
    let n = files.len() as f64;
    let df = |needle: &str| {
        files
            .iter()
            .filter(|(path, content)| fold(path).contains(needle) || content.contains(needle))
            .count() as f64
    };
    let idf: HashMap<&str, f64> = stems
        .iter()
        .map(|s| (s.as_str(), ((n + 1.0) / (df(s) + 1.0)).ln()))
        .collect();
    let bigrams: Vec<(Vec<String>, f64)> = stems
        .windows(2)
        .map(|w| {
            let forms = vec![
                format!("{}_{}", w[0], w[1]),
                format!("{}{}", w[0], w[1]),
                format!("{}-{}", w[0], w[1]),
            ];
            (forms, idf[w[0].as_str()].max(idf[w[1].as_str()]))
        })
        .collect();

    let mut scores: Vec<(String, f64)> = files
        .iter()
        .map(|(path, content)| {
            let path_folded = fold(path);
            let mut score = 0.0;
            for s in &stems {
                let weight = idf[s.as_str()];
                if path_folded.contains(s.as_str()) {
                    score += 3.0 * weight;
                }
                let hits = content.matches(s.as_str()).count();
                if hits > 0 {
                    score += weight * if hits >= 3 { 1.5 } else { 1.0 };
                }
            }
            for (forms, weight) in &bigrams {
                if forms.iter().any(|f| content.contains(f.as_str())) {
                    score += 4.0 * weight;
                }
            }
            (path.clone(), score)
        })
        .collect();
    scores.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    scores
}

/// Strip one common English suffix so "pagination" matches "paginate" and
/// "products" matches "product".
fn stem(word: &str) -> String {
    for suffix in [
        "ations", "ation", "ions", "ion", "ings", "ing", "ers", "er", "es", "s", "ed",
    ] {
        if let Some(base) = word.strip_suffix(suffix) {
            if base.chars().count() >= 4 {
                return base.to_string();
            }
        }
    }
    word.to_string()
}

fn is_test_file(path: &str) -> bool {
    let lower = path.to_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    lower
        .split('/')
        .any(|seg| matches!(seg, "test" | "tests" | "spec" | "__tests__"))
        || name.starts_with("test_")
        || name.contains("_test.")
        || name.contains(".test.")
        || name.contains(".spec.")
}

/// Tracked files (via git) or, outside git, a bounded walk skipping hidden
/// and build directories. Paths are relative, with `/` separators.
pub fn project_files(dir: &Path) -> Vec<String> {
    let tracked = Command::new("git")
        .arg("-C")
        .arg(dir)
        .arg("ls-files")
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .take(MAX_FILES)
                .map(str::to_string)
                .collect::<Vec<_>>()
        });
    if let Some(files) = tracked.filter(|f| !f.is_empty()) {
        return files;
    }
    let mut files = Vec::new();
    walk(dir, dir, &mut files);
    files.sort();
    files
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        if out.len() >= MAX_FILES {
            return;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            if !SKIP_DIRS.contains(&name.as_str()) {
                walk(root, &path, out);
            }
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn repo(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (path, content) in files {
            let p = dir.path().join(path);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, content).unwrap();
        }
        dir
    }

    fn shop() -> tempfile::TempDir {
        repo(&[
            (
                "shop/pagination.py",
                "def paginate(items, page, size):\n    start = (page - 1) * size\n",
            ),
            (
                "shop/report.py",
                "def revenue_by_day(orders):\n    pass\ndef top_products(orders):\n    pass\n",
            ),
            (
                "shop/inventory.py",
                "def low_stock(self, threshold=5):\n    return [s for s in self._stock]\n",
            ),
            (
                "tests/test_pagination.py",
                "from shop.pagination import paginate\n",
            ),
        ])
    }

    #[test]
    fn names_the_file_a_vague_request_is_about() {
        let dir = shop();
        assert_eq!(
            likely_target("pagination is broken, can you fix it", dir.path()).as_deref(),
            Some("shop/pagination.py")
        );
        assert_eq!(
            likely_target("the top products widget is wrong", dir.path()).as_deref(),
            Some("shop/report.py")
        );
    }

    #[test]
    fn no_target_without_content_words_or_clear_winner() {
        let dir = shop();
        assert_eq!(
            likely_target("the tests are failing, fix them", dir.path()),
            None
        );
        assert_eq!(likely_target("fix the bug", dir.path()), None);
    }

    #[test]
    fn test_files_are_never_targets() {
        assert!(is_test_file("tests/test_users.py"));
        assert!(is_test_file("src/cart.test.ts"));
        assert!(is_test_file("pkg/cart_test.go"));
        assert!(!is_test_file("shop/latest.py"));
    }

    #[test]
    fn stems_common_suffixes() {
        assert_eq!(stem("pagination"), "pagin");
        assert_eq!(stem("products"), "product");
        assert_eq!(stem("bus"), "bus");
    }

    #[test]
    fn trims_a_pasted_log_to_its_first_error() {
        let mut log = String::new();
        for i in 0..50 {
            log.push_str(&format!("2026-10-30T14:00:{i:02}Z INFO request ok\n"));
        }
        log.push_str("2026-10-30T14:01:00Z ERROR checkout failed\nTraceback (most recent call last):\n  File \"shop/pricing.py\", line 25, in order_total\n    raise ValueError(\"negative\")\nValueError: negative\n");
        log.push_str("2026-10-30T14:01:01Z INFO request ok\n");
        let prompt = format!("checkout is failing, logs:\n\n{log}\n\nfix it");
        let (text, removed) = trim_pastes(&prompt, "First error ({n} lines):");
        assert!(text.starts_with("checkout is failing, logs:"));
        assert!(text.contains("ERROR checkout failed") && text.contains("ValueError: negative"));
        assert!(!text.contains("14:00:10Z INFO"), "INFO noise removed");
        assert!(text.ends_with("fix it"));
        assert!(removed > 1_500, "removed {removed} bytes");
    }

    #[test]
    fn pastes_without_errors_are_kept() {
        let paste = "line of config\n".repeat(30);
        let (text, removed) = trim_pastes(&format!("look at this\n\n{paste}"), "n/a");
        assert_eq!(removed, 0);
        assert!(text.contains(paste.trim_end()));
    }

    #[test]
    fn rewrite_local_only_changes_what_it_can_ground() {
        let dir = shop();
        let r = rewrite_local("pagination is broken, can you fix it", dir.path());
        assert!(r.changed);
        assert!(
            r.text
                .starts_with("pagination is broken, can you fix it\n\n")
        );
        assert!(r.text.contains("`shop/pagination.py`"));

        let untouched = rewrite_local("the tests are failing, fix them", dir.path());
        assert!(!untouched.changed);
        assert_eq!(untouched.text, "the tests are failing, fix them");
    }

    #[test]
    fn excerpt_naming_a_file_needs_no_extra_hint() {
        let dir = shop();
        let r = rewrite_local("fix the crash in shop/pagination.py", dir.path());
        assert_eq!(r.target, None);
        assert!(!r.changed);
    }

    #[test]
    fn hint_is_localized() {
        let dir = shop();
        let r = rewrite_local("διόρθωσε το pagination", dir.path());
        assert!(r.text.contains("Πιθανή θέση"), "{}", r.text);
    }
}
