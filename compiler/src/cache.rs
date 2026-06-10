//! Incremental Compilation Cache
//!
//! Provides hash-based dependency tracking and caching
//! for fast rebuilds when only some files change.
//!
//! Cache structure:
//! ```text
//! build/.cache/
//!   manifest.json     - Maps behavior name -> cache entry
//!   objects/          - Per-behavior object files
//! ```

use sha2::{Sha256, Digest};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Cache manifest version - increment when format changes
const MANIFEST_VERSION: u32 = 1;

/// Cache manifest - tracks all cached behaviors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheManifest {
    pub version: u32,
    /// Identifies the compiler build that produced these objects. When the
    /// compiler itself changes (e.g. a codegen edit) this differs from the
    /// running compiler's id, so the whole cache is invalidated — otherwise a
    /// rebuilt compiler would silently reuse stale object files.
    #[serde(default)]
    pub compiler_id: String,
    pub behaviors: HashMap<String, CacheEntry>,
}

/// Cache entry for a single behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// Path to source file (for display)
    pub source_path: PathBuf,
    /// SHA256 hash of source file content
    pub source_hash: String,
    /// Contract hash from behavior's HASH field
    pub contract_hash: String,
    /// Combined hash of all dependency contract hashes
    pub deps_hash: String,
    /// Path to cached object file
    pub object_path: PathBuf,
    /// Unix timestamp of last compilation
    pub timestamp: u64,
}

impl CacheManifest {
    /// Create a new empty manifest
    pub fn new() -> Self {
        CacheManifest {
            version: MANIFEST_VERSION,
            compiler_id: compiler_build_id(),
            behaviors: HashMap::new(),
        }
    }

    /// Load manifest from cache directory, or create new if not exists
    pub fn load(cache_dir: &Path) -> Self {
        let manifest_path = cache_dir.join("manifest.json");

        if !manifest_path.exists() {
            return Self::new();
        }

        match fs::read_to_string(&manifest_path) {
            Ok(content) => {
                match serde_json::from_str::<CacheManifest>(&content) {
                    Ok(manifest) => {
                        // Check version compatibility
                        if manifest.version != MANIFEST_VERSION {
                            eprintln!("Cache version mismatch, rebuilding...");
                            return Self::new();
                        }
                        // Invalidate if the compiler itself changed since this
                        // cache was written (stale objects would otherwise be reused).
                        if manifest.compiler_id != compiler_build_id() {
                            eprintln!("Compiler changed since last build, rebuilding...");
                            return Self::new();
                        }
                        manifest
                    }
                    Err(e) => {
                        eprintln!("Failed to parse cache manifest: {}", e);
                        Self::new()
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to read cache manifest: {}", e);
                Self::new()
            }
        }
    }

    /// Save manifest to cache directory
    pub fn save(&self, cache_dir: &Path) -> Result<(), String> {
        // Ensure cache directory exists
        fs::create_dir_all(cache_dir)
            .map_err(|e| format!("Failed to create cache directory: {}", e))?;

        // Ensure objects subdirectory exists
        fs::create_dir_all(cache_dir.join("objects"))
            .map_err(|e| format!("Failed to create objects directory: {}", e))?;

        let manifest_path = cache_dir.join("manifest.json");
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize manifest: {}", e))?;

        let mut file = fs::File::create(&manifest_path)
            .map_err(|e| format!("Failed to create manifest file: {}", e))?;
        file.write_all(content.as_bytes())
            .map_err(|e| format!("Failed to write manifest: {}", e))?;

        Ok(())
    }

    /// Check if a behavior needs recompilation
    pub fn needs_recompile(
        &self,
        name: &str,
        source_hash: &str,
        contract_hash: &str,
        deps_hash: &str,
    ) -> bool {
        match self.behaviors.get(name) {
            None => true, // Not in cache
            Some(entry) => {
                // Check if source changed
                if entry.source_hash != source_hash {
                    return true;
                }
                // Check if contract changed
                if entry.contract_hash != contract_hash {
                    return true;
                }
                // Check if dependencies changed
                if entry.deps_hash != deps_hash {
                    return true;
                }
                // Check if object file exists
                if !entry.object_path.exists() {
                    return true;
                }
                false
            }
        }
    }

    /// Update or insert a cache entry
    pub fn update_entry(&mut self, name: &str, entry: CacheEntry) {
        self.behaviors.insert(name.to_string(), entry);
    }

    /// Get the object path for a cached behavior
    #[allow(dead_code)]
    pub fn get_object_path(&self, name: &str) -> Option<&PathBuf> {
        self.behaviors.get(name).map(|e| &e.object_path)
    }

    /// Remove a behavior from cache
    #[allow(dead_code)]
    pub fn remove(&mut self, name: &str) {
        if let Some(entry) = self.behaviors.remove(name) {
            // Try to delete the object file
            let _ = fs::remove_file(&entry.object_path);
        }
    }

    /// Clear all cached behaviors
    #[allow(dead_code)]
    pub fn clear(&mut self, cache_dir: &Path) {
        // Remove all object files
        let objects_dir = cache_dir.join("objects");
        if objects_dir.exists() {
            let _ = fs::remove_dir_all(&objects_dir);
        }
        self.behaviors.clear();
    }
}

impl Default for CacheManifest {
    fn default() -> Self {
        Self::new()
    }
}

/// Identify the running compiler build, so cached objects are invalidated when
/// the compiler changes (e.g. a codegen edit) even if the Sigil sources did not.
/// Derived from the compiler executable's length + modification time (a relink
/// changes both); falls back to the crate version if the exe can't be inspected.
pub fn compiler_build_id() -> String {
    let version = env!("CARGO_PKG_VERSION");
    if let Ok(exe) = std::env::current_exe() {
        if let Ok(meta) = fs::metadata(&exe) {
            let mtime = meta.modified().ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            return format!("{}-{}-{}", version, meta.len(), mtime);
        }
    }
    version.to_string()
}

/// Compute SHA256 hash of a file's contents
pub fn compute_source_hash(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path)
        .map_err(|e| format!("Failed to open {}: {}", path.display(), e))?;

    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = file.read(&mut buffer)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hex::encode(hasher.finalize()))
}

/// Compute combined hash of all dependency contract hashes
///
/// The deps_hash changes if any dependency's contract changes,
/// triggering recompilation of dependents.
pub fn compute_deps_hash(dep_hashes: &[(&str, &str)]) -> String {
    if dep_hashes.is_empty() {
        return "none".to_string();
    }

    let mut hasher = Sha256::new();

    // Sort by name for deterministic ordering
    let mut sorted: Vec<_> = dep_hashes.iter().collect();
    sorted.sort_by_key(|(name, _)| *name);

    for (name, hash) in sorted {
        hasher.update(name.as_bytes());
        hasher.update(b"@");
        hasher.update(hash.as_bytes());
        hasher.update(b"\n");
    }

    hex::encode(&hasher.finalize()[..8]) // 16 hex chars
}

/// Get current Unix timestamp
pub fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Generate object file path from behavior name and source hash
pub fn object_path(cache_dir: &Path, name: &str, source_hash: &str) -> PathBuf {
    // Use first 16 chars of hash for uniqueness
    let short_hash = &source_hash[..16.min(source_hash.len())];
    cache_dir.join("objects").join(format!("{}_{}.o", name, short_hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_new_manifest() {
        let manifest = CacheManifest::new();
        assert_eq!(manifest.version, MANIFEST_VERSION);
        assert!(manifest.behaviors.is_empty());
    }

    #[test]
    fn test_manifest_save_load() {
        let dir = tempdir().unwrap();
        let cache_dir = dir.path();

        let mut manifest = CacheManifest::new();
        manifest.update_entry("test_behavior", CacheEntry {
            source_path: PathBuf::from("test.beh"),
            source_hash: "abc123".to_string(),
            contract_hash: "def456".to_string(),
            deps_hash: "none".to_string(),
            object_path: cache_dir.join("objects/test.o"),
            timestamp: 12345,
        });

        manifest.save(cache_dir).unwrap();

        let loaded = CacheManifest::load(cache_dir);
        assert_eq!(loaded.version, MANIFEST_VERSION);
        assert!(loaded.behaviors.contains_key("test_behavior"));

        let entry = loaded.behaviors.get("test_behavior").unwrap();
        assert_eq!(entry.source_hash, "abc123");
        assert_eq!(entry.contract_hash, "def456");
    }

    #[test]
    fn test_compiler_change_invalidates_cache() {
        let dir = tempdir().unwrap();
        let cache_dir = dir.path();

        // Write a manifest that claims a different compiler build produced it.
        let mut manifest = CacheManifest::new();
        manifest.compiler_id = "some-old-compiler-build".to_string();
        manifest.update_entry("test_behavior", CacheEntry {
            source_path: PathBuf::from("test.beh"),
            source_hash: "abc123".to_string(),
            contract_hash: "def456".to_string(),
            deps_hash: "none".to_string(),
            object_path: cache_dir.join("objects/test.o"),
            timestamp: 12345,
        });
        manifest.save(cache_dir).unwrap();

        // Loading with the current (different) compiler id must drop the entries.
        let loaded = CacheManifest::load(cache_dir);
        assert!(loaded.behaviors.is_empty(),
            "cache from a different compiler build must be invalidated");
        assert_eq!(loaded.compiler_id, compiler_build_id());
    }

    #[test]
    fn test_needs_recompile_not_cached() {
        let manifest = CacheManifest::new();
        assert!(manifest.needs_recompile("unknown", "hash1", "hash2", "hash3"));
    }

    #[test]
    fn test_needs_recompile_source_changed() {
        let dir = tempdir().unwrap();
        let obj_path = dir.path().join("test.o");
        fs::write(&obj_path, b"fake object").unwrap();

        let mut manifest = CacheManifest::new();
        manifest.update_entry("test", CacheEntry {
            source_path: PathBuf::from("test.beh"),
            source_hash: "old_hash".to_string(),
            contract_hash: "contract".to_string(),
            deps_hash: "deps".to_string(),
            object_path: obj_path,
            timestamp: 0,
        });

        // Different source hash triggers recompile
        assert!(manifest.needs_recompile("test", "new_hash", "contract", "deps"));
    }

    #[test]
    fn test_needs_recompile_deps_changed() {
        let dir = tempdir().unwrap();
        let obj_path = dir.path().join("test.o");
        fs::write(&obj_path, b"fake object").unwrap();

        let mut manifest = CacheManifest::new();
        manifest.update_entry("test", CacheEntry {
            source_path: PathBuf::from("test.beh"),
            source_hash: "source".to_string(),
            contract_hash: "contract".to_string(),
            deps_hash: "old_deps".to_string(),
            object_path: obj_path,
            timestamp: 0,
        });

        // Different deps hash triggers recompile
        assert!(manifest.needs_recompile("test", "source", "contract", "new_deps"));
    }

    #[test]
    fn test_needs_recompile_object_missing() {
        let mut manifest = CacheManifest::new();
        manifest.update_entry("test", CacheEntry {
            source_path: PathBuf::from("test.beh"),
            source_hash: "source".to_string(),
            contract_hash: "contract".to_string(),
            deps_hash: "deps".to_string(),
            object_path: PathBuf::from("/nonexistent/path.o"),
            timestamp: 0,
        });

        // Missing object file triggers recompile
        assert!(manifest.needs_recompile("test", "source", "contract", "deps"));
    }

    #[test]
    fn test_needs_recompile_cached_valid() {
        let dir = tempdir().unwrap();
        let obj_path = dir.path().join("test.o");
        fs::write(&obj_path, b"fake object").unwrap();

        let mut manifest = CacheManifest::new();
        manifest.update_entry("test", CacheEntry {
            source_path: PathBuf::from("test.beh"),
            source_hash: "source".to_string(),
            contract_hash: "contract".to_string(),
            deps_hash: "deps".to_string(),
            object_path: obj_path,
            timestamp: 0,
        });

        // All match, no recompile needed
        assert!(!manifest.needs_recompile("test", "source", "contract", "deps"));
    }

    #[test]
    fn test_compute_source_hash() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(b"hello world").unwrap();

        let hash = compute_source_hash(&file_path).unwrap();
        assert_eq!(hash.len(), 64); // SHA256 = 64 hex chars

        // Same content = same hash
        let hash2 = compute_source_hash(&file_path).unwrap();
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_compute_deps_hash_empty() {
        let hash = compute_deps_hash(&[]);
        assert_eq!(hash, "none");
    }

    #[test]
    fn test_compute_deps_hash_deterministic() {
        let deps = vec![
            ("foo", "abc123"),
            ("bar", "def456"),
        ];
        let hash1 = compute_deps_hash(&deps);

        // Order shouldn't matter (sorted internally)
        let deps2 = vec![
            ("bar", "def456"),
            ("foo", "abc123"),
        ];
        let hash2 = compute_deps_hash(&deps2);

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_compute_deps_hash_changes() {
        let deps1 = vec![("foo", "abc123")];
        let deps2 = vec![("foo", "xyz789")];

        let hash1 = compute_deps_hash(&deps1);
        let hash2 = compute_deps_hash(&deps2);

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_object_path() {
        let cache_dir = Path::new("build/.cache");
        let path = object_path(cache_dir, "println", "abcdef1234567890abcdef");

        assert!(path.to_string_lossy().contains("println"));
        assert!(path.to_string_lossy().contains("abcdef1234567890"));
        assert!(path.to_string_lossy().ends_with(".o"));
    }
}
