use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use parking_lot::Mutex;

pub trait FileSystem: Send + Sync {
    fn read_to_string(&self, path: &Path) -> std::io::Result<String>;
    fn write_string(&self, path: &Path, content: &str) -> std::io::Result<()>;
    fn exists(&self, path: &Path) -> bool;
    fn read_dir_names(&self, path: &Path) -> std::io::Result<Vec<String>>;
    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf>;
    fn mkdir(&self, path: &Path) -> std::io::Result<()>;
    fn remove(&self, path: &Path) -> std::io::Result<()>;
    fn copy_file(&self, src: &Path, dst: &Path) -> std::io::Result<()>;
    fn move_path(&self, src: &Path, dst: &Path) -> std::io::Result<()>;
    fn glob(&self, pattern: &str) -> std::io::Result<Vec<String>>;
    fn mktemp(&self, is_dir: bool) -> std::io::Result<String>;
}

#[derive(Default)]
pub struct NativeFileSystem;

fn ensure_parent(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

impl FileSystem for NativeFileSystem {
    fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn write_string(&self, path: &Path, content: &str) -> std::io::Result<()> {
        ensure_parent(path)?;
        std::fs::write(path, content)
    }

    fn exists(&self, path: &Path) -> bool {
        std::fs::metadata(path).is_ok()
    }

    fn read_dir_names(&self, path: &Path) -> std::io::Result<Vec<String>> {
        let mut names = Vec::new();
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            if let Ok(name) = entry.file_name().into_string() {
                names.push(name);
            }
        }
        Ok(names)
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path).or_else(|_| std::path::absolute(path))
    }

    fn mkdir(&self, path: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(path)
    }

    fn remove(&self, path: &Path) -> std::io::Result<()> {
        let meta = std::fs::metadata(path)?;
        if meta.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        }
    }

    fn copy_file(&self, src: &Path, dst: &Path) -> std::io::Result<()> {
        ensure_parent(dst)?;
        std::fs::copy(src, dst).map(|_| ())
    }

    fn move_path(&self, src: &Path, dst: &Path) -> std::io::Result<()> {
        ensure_parent(dst)?;
        std::fs::rename(src, dst)
    }

    fn glob(&self, pattern: &str) -> std::io::Result<Vec<String>> {
        let entries = ::glob::glob(pattern)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e.msg))?;
        let mut paths = Vec::new();
        for entry in entries {
            let p = entry.map_err(|e| std::io::Error::other(e.to_string()))?;
            if let Some(s) = p.to_str() {
                paths.push(s.to_string());
            }
        }
        Ok(paths)
    }

    fn mktemp(&self, is_dir: bool) -> std::io::Result<String> {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("keel-{pid}-{n}"));
        if is_dir {
            std::fs::create_dir(&path)?;
        } else {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
        }
        Ok(path.to_string_lossy().into_owned())
    }
}

#[allow(dead_code)]
#[derive(Default)]
pub struct InMemoryFileSystem {
    files: Mutex<HashMap<PathBuf, String>>,
    dirs: Mutex<BTreeSet<PathBuf>>,
}

impl InMemoryFileSystem {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(dead_code)]
    fn normalize(path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            PathBuf::from("/").join(path)
        }
    }
}

impl FileSystem for InMemoryFileSystem {
    fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        let path = Self::normalize(path);
        self.files
            .lock()
            .get(&path)
            .cloned()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"))
    }

    fn write_string(&self, path: &Path, content: &str) -> std::io::Result<()> {
        let path = Self::normalize(path);
        if let Some(parent) = path.parent() {
            self.dirs.lock().insert(parent.to_path_buf());
        }
        self.files.lock().insert(path, content.to_string());
        Ok(())
    }

    fn exists(&self, path: &Path) -> bool {
        let path = Self::normalize(path);
        self.files.lock().contains_key(&path) || self.dirs.lock().contains(&path)
    }

    fn read_dir_names(&self, path: &Path) -> std::io::Result<Vec<String>> {
        let path = Self::normalize(path);
        let mut names = BTreeSet::new();
        for file in self.files.lock().keys() {
            if let Some(parent) = file.parent()
                && parent == path
                && let Some(name) = file.file_name().and_then(|n| n.to_str())
            {
                names.insert(name.to_string());
            }
        }
        if names.is_empty() && !self.dirs.lock().contains(&path) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "directory not found",
            ));
        }
        Ok(names.into_iter().collect())
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        Ok(Self::normalize(path))
    }

    fn mkdir(&self, path: &Path) -> std::io::Result<()> {
        let path = Self::normalize(path);
        // Insert the directory and all its ancestors.
        let mut dirs = self.dirs.lock();
        let mut current = path.as_path();
        loop {
            dirs.insert(current.to_path_buf());
            match current.parent() {
                Some(p) if p != current => current = p,
                _ => break,
            }
        }
        Ok(())
    }

    fn remove(&self, path: &Path) -> std::io::Result<()> {
        let path = Self::normalize(path);
        let mut files = self.files.lock();
        let mut dirs = self.dirs.lock();
        // Remove a single file.
        if files.remove(&path).is_some() {
            return Ok(());
        }
        // Remove a directory and everything under it (rm -rf semantics).
        if dirs.contains(&path) {
            let file_keys: Vec<PathBuf> = files
                .keys()
                .filter(|k| k.starts_with(&path))
                .cloned()
                .collect();
            for k in file_keys {
                files.remove(&k);
            }
            let dir_keys: Vec<PathBuf> = dirs
                .iter()
                .filter(|k| k.starts_with(&path))
                .cloned()
                .collect();
            for k in dir_keys {
                dirs.remove(&k);
            }
            return Ok(());
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "path not found",
        ))
    }

    fn copy_file(&self, src: &Path, dst: &Path) -> std::io::Result<()> {
        let src = Self::normalize(src);
        let dst = Self::normalize(dst);
        let content =
            self.files.lock().get(&src).cloned().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "source not found")
            })?;
        if let Some(parent) = dst.parent() {
            self.dirs.lock().insert(parent.to_path_buf());
        }
        self.files.lock().insert(dst, content);
        Ok(())
    }

    fn move_path(&self, src: &Path, dst: &Path) -> std::io::Result<()> {
        let src = Self::normalize(src);
        let dst = Self::normalize(dst);
        let content =
            self.files.lock().remove(&src).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "source not found")
            })?;
        if let Some(parent) = dst.parent() {
            self.dirs.lock().insert(parent.to_path_buf());
        }
        self.files.lock().insert(dst, content);
        Ok(())
    }

    fn glob(&self, pattern: &str) -> std::io::Result<Vec<String>> {
        let pat = ::glob::Pattern::new(pattern)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e.msg))?;
        let files = self.files.lock();
        let mut matched: Vec<String> = files
            .keys()
            .filter_map(|p| {
                let s = p.to_str()?;
                // Match against the full absolute path or, for relative patterns,
                // also strip the leading `/` prefix used by InMemoryFileSystem.
                let candidate = s.trim_start_matches('/');
                if pat.matches(s) || pat.matches(candidate) {
                    Some(candidate.to_string())
                } else {
                    None
                }
            })
            .collect();
        matched.sort();
        Ok(matched)
    }

    fn mktemp(&self, is_dir: bool) -> std::io::Result<String> {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = PathBuf::from(format!("/tmp/keel-mock-{n}"));
        if is_dir {
            self.dirs.lock().insert(path.clone());
        } else {
            self.files.lock().insert(path.clone(), String::new());
        }
        Ok(path.to_string_lossy().into_owned())
    }
}
