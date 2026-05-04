 v0.1.11 Memory Storage Plan — Safest Implementation                                                                                                                                                                                               
                                                                                                        
  Safety axes addressed                                                                                                                                                                                                                             
   
  1. Identity uniqueness — 48-bit hash + canonical path                                                                                                                                                                                             
  2. Cross-process consistency — flock advisory file locking                                            
  3. Crash durability — fsync data + parent dir                                                                                                                                                                                                     
  4. Lock-rename hazard — sidecar .lock file, never renamed                                                                                                                                                                                         
  5. Read concurrency — shared lock for recall, exclusive for remember/forget                                                                                                                                                                       
  6. Path traversal — hard runtime validation, not debug_assert                                                                                                                                                                                     
  7. Non-UTF-8 paths — hash raw bytes via OsStr::as_encoded_bytes(), no lossy conversion                                                                                                                                                            
  8. Canonicalize failure — fall back to std::path::absolute()                                                                                                                                                                                      
  9. TOCTOU on lockfile — accept (lock is on inode; file persists in user-owned dir)                                                                                                                                                                
  10. Stale .tmp — clean up on rename failure (already done) and at lock acquisition (new)                                                                                                                                                          
                                                                                                                                                                                                                                                    
  Identity algorithm                                                                                                                                                                                                                                
                                                                                                                                                                                                                                                    
  ~/.keel/memory/<basename>_<hash12>/<agent>.json                                                                                                                                                                                                   
  ~/.keel/memory/<basename>_<hash12>/<agent>.lock                                                       
                                                                                                                                                                                                                                                    
  fn derive_program_name(raw_source_name: &str) -> String {
      let path = Path::new(raw_source_name);                                                                                                                                                                                                        
                                                                                                        
      // Try canonicalize → fall back to absolute → fall back to raw bytes.                                                                                                                                                                         
      let resolved: Option<PathBuf> = std::fs::canonicalize(path).ok()                                  
          .or_else(|| std::path::absolute(path).ok());                                                                                                                                                                                              
                                                                                                                                                                                                                                                    
      match resolved {                         
          Some(p) if p.is_file() || p.is_symlink() => {                                                                                                                                                                                             
              // Hash the OsStr bytes directly — no lossy UTF-8 conversion.                             
              let bytes = p.as_os_str().as_encoded_bytes();                                                                                                                                                                                         
              let mut h = Sha256::new();                                                                                                                                                                                                            
              h.update(bytes);                                                                                                                                                                                                                      
              let hex = format!("{:x}", h.finalize());                                                                                                                                                                                              
              let hash12 = &hex[..12];                                                                  
                                                                                                                                                                                                                                                    
              let basename = p.file_stem()                                                                                                                                                                                                          
                  .and_then(|s| s.to_str())    
                  .unwrap_or("program");                                                                                                                                                                                                            
              let safe_basename = sanitize_basename(basename);                                          
              format!("{safe_basename}_{hash12}")                                                                                                                                                                                                   
          }
          _ => {                                                                                                                                                                                                                                    
              // REPL / stdin / inline / non-existent → ephemeral namespace.                            
              // We DO NOT hash here because there's no stable identity to hash.                                                                                                                                                                    
              // All REPL sessions share __repl__; all inline runs share __inline__.                                                                                                                                                                
              // This matches user expectation for exploratory contexts.                                                                                                                                                                            
              classify_special_source(raw_source_name)                                                                                                                                                                                              
          }                                                                                                                                                                                                                                         
      }                                                                                                                                                                                                                                             
  }                                                                                                                                                                                                                                                 
                                                                                                        
  fn sanitize_basename(s: &str) -> String {    
      let cleaned: String = s.chars()
          .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })                                                                                                                                                                     
          .take(64)
          .collect();                                                                                                                                                                                                                               
      if cleaned.is_empty() { "program".into() } else { cleaned }                                       
  }                                                                                                                                                                                                                                                 
                                                                                                        
  fn classify_special_source(raw: &str) -> String {
      match raw {
          "<repl>" | "repl" => "__repl__".into(),                                                                                                                                                                                                   
          "<stdin>" | "stdin" => "__stdin__".into(),
          "<inline>" | "inline" => "__inline__".into(),                                                                                                                                                                                             
          other => {                                                                                    
              let safe = sanitize_basename(other);                                                                                                                                                                                                  
              format!("__{safe}__")                                                                                                                                                                                                                 
          }                                    
      }                                                                                                                                                                                                                                             
  }                                                                                                     
                                               
  Hash choice: SHA-256 truncated to 12 hex chars (48 bits). Birthday collision probability hits 1% at ~16.7M distinct files. SHA-256 is overkill cryptographically but mature and stable forever. No external crate beyond sha2 = "0.10" (16 KB     
  compiled).
                                                                                                                                                                                                                                                    
  Locking protocol                                                                                      
                                               
  // Pseudo-code for the critical section in remember/recall/forget.
                                                                                                                                                                                                                                                    
  fn with_locked<R>(
      program: &str,                                                                                                                                                                                                                                
      agent: &str,                                                                                                                                                                                                                                  
      exclusive: bool,                         
      body: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>) -> Result<R>,                                                                                                                                                              
  ) -> Result<R> {                                                                                                                                                                                                                                  
      let dir = persistent_memory_dir(program);
      fs::create_dir_all(&dir)?;                                                                                                                                                                                                                    
                                                                                                        
      let lock_path = dir.join(format!("{agent}.lock"));                                                                                                                                                                                            
      let lock_file = OpenOptions::new()                                                                
          .read(true).write(true).create(true)                                                                                                                                                                                                      
          .open(&lock_path)?;                                                                                                                                                                                                                       
                                               
      if exclusive {                                                                                                                                                                                                                                
          lock_file.lock_exclusive()?;   // fs2::FileExt                                                
      } else {                                                                                                                                                                                                                                      
          lock_file.lock_shared()?;
      }                                                                                                                                                                                                                                             
      // Lock auto-releases on drop, including panic unwinding.                                                                                                                                                                                     
                                                                                                                                                                                                                                                    
      // Clean stale .tmp from a previous crashed run (we hold the lock now).                                                                                                                                                                       
      let data_path = dir.join(format!("{agent}.json"));                                                                                                                                                                                            
      let tmp_path = data_path.with_extension("json.tmp");                                                                                                                                                                                          
      let _ = fs::remove_file(&tmp_path);                                                                                                                                                                                                           
                                               
      let mut store = read_or_empty(&data_path)?;                                                                                                                                                                                                   
      let result = body(&mut store)?;                                                                   
                                                                                                                                                                                                                                                    
      if exclusive {                                                                                                                                                                                                                                
          write_atomic(&data_path, &store)?;  // .tmp + fsync + rename + fsync(parent)
      }                                                                                                                                                                                                                                             
      Ok(result)                                                                                        
  }                                                                                                                                                                                                                                                 
                                                                                                        
  Why a sidecar .lock rather than locking the data file: the data file is renamed on every write. A lock held against the old inode no longer protects the new file's inode, so a second process could miss the lock between rename and close. The  
  sidecar lockfile is never renamed, so the lock target is stable.                                      
                                                                                                                                                                                                                                                    
  Drop PERSIST_LOCK: flock is per-fd on Linux and per-process on BSD/macOS. Each call site opens its own fd, so flock serializes both within and across processes. The in-process mutex is redundant; remove it.                                    
                                               
  Atomic write hardening                                                                                                                                                                                                                            
                                                                                                        
  fn write_atomic(path: &Path, store: &serde_json::Map<...>) -> Result<()> {
      let tmp = path.with_extension("json.tmp");                                                                                                                                                                                                    
      let json = serde_json::to_string_pretty(&Value::Object(store.clone()))?;
                                                                                                                                                                                                                                                    
      {                                                                                                 
          let mut f = OpenOptions::new()                                                                                                                                                                                                            
              .create(true).write(true).truncate(true).open(&tmp)?;                                                                                                                                                                                 
          f.write_all(json.as_bytes())?;       
          f.sync_all()?;  // fsync data + metadata before rename                                                                                                                                                                                    
      }                                                                                                                                                                                                                                             
                                               
      if let Err(e) = fs::rename(&tmp, path) {                                                                                                                                                                                                      
          let _ = fs::remove_file(&tmp);                                                                
          return Err(e.into());                                                                                                                                                                                                                     
      }                                                                                                 
                                               
      // fsync parent dir so the rename itself is durable.                                                                                                                                                                                          
      // No-op semantics on Windows; OpenOptions::open on a directory works on Unix.
      if let Some(parent) = path.parent() {                                                                                                                                                                                                         
          if let Ok(d) = File::open(parent) {                                                           
              let _ = d.sync_all();  // best-effort; ignore EINVAL on platforms that disallow                                                                                                                                                       
          }                                                                                                                                                                                                                                         
      }                                                                                                                                                                                                                                             
      Ok(())                                                                                                                                                                                                                                        
  }                                                                                                     
                                               
  Validation tightening                                                                                                                                                                                                                             
  
  debug_assert! on agent names is removed. Replaced with a hard check at the boundary of persistent_memory_path:                                                                                                                                    
                                                                                                        
  fn validate_path_component(name: &str, kind: &str) -> Result<()> {                                                                                                                                                                                
      if name.is_empty()                                                                                                                                                                                                                            
          || name == "." || name == ".."       
          || name.contains('/') || name.contains('\\') || name.contains('\0')                                                                                                                                                                       
      {                                                                                                 
          return Err(miette::miette!("Memory: invalid {kind} name {name:?}"));                                                                                                                                                                      
      }                                                                                                                                                                                                                                             
      Ok(())                                   
  }                                                                                                                                                                                                                                                 
                                                                                                        
  This protects against future identifier-rule relaxation and any input that bypasses the parser (e.g. a future Memory.remember_for(agent_name, ...) API).                                                                                          
  
  Dependencies to add                                                                                                                                                                                                                               
                                                                                                        
  sha2 = "0.10"   # ~16 KB, stable, no transitive deps beyond `digest` and `block-buffer`
  fs2 = "0.4"     # ~10 KB, cross-platform flock/LockFileEx wrapper                                                                                                                                                                                 
                                                                                                                                                                                                                                                    
  Both are mature, low-churn, widely used (sha2: 90M downloads/yr; fs2: 50M).                                                                                                                                                                       
                                                                                                                                                                                                                                                    
  Alternative for fs2: hand-roll a ~30-line wrapper using libc::flock on Unix and windows-sys::LockFileEx on Windows. Adds platform-conditional code; saves a dependency. Recommendation: use fs2 for v0.1.11, vendor later if dep churn becomes a  
  concern.                                                                                              
                                                                                                                                                                                                                                                    
  Migration                                                                                             
                                               
  v0.1.10 shipped today. Existing data: probably none in the wild.                                                                                                                                                                                  
  
  Strategy: hard break; document clearly.                                                                                                                                                                                                           
                                                                                                        
  CHANGELOG entry:                                                                                                                                                                                                                                  
  ### Breaking                                                                                          
  - Persistent memory directory layout changed from                                                                                                                                                                                                 
    `~/.keel/memory/<file-stem>/<agent>.json` to                                                        
    `~/.keel/memory/<basename>_<hash12>/<agent>.json` for path safety                                                                                                                                                                               
    across same-named files. Existing data is not auto-migrated; move
    manually if needed.                                                                                                                                                                                                                             
                                                                                                                                                                                                                                                    
  Tests                                                                                                                                                                                                                                             
                                                                                                                                                                                                                                                    
  ┌────────────────────────────────────────────────┬─────────────────────────────────────────────────────────────────────────────────┐                                                                                                              
  │                      Test                      │                                     Asserts                                     │
  ├────────────────────────────────────────────────┼─────────────────────────────────────────────────────────────────────────────────┤                                                                                                              
  │ memory_isolation_same_basename_different_paths │ Two counter.keel files in different temp dirs maintain separate state           │
  ├────────────────────────────────────────────────┼─────────────────────────────────────────────────────────────────────────────────┤
  │ memory_persistence_same_path_across_runs       │ Existing test — adapted for canonicalize                                        │                                                                                                              
  ├────────────────────────────────────────────────┼─────────────────────────────────────────────────────────────────────────────────┤                                                                                                              
  │ memory_symlink_resolves_to_same_storage        │ Symlink and target share memory                                                 │                                                                                                              
  ├────────────────────────────────────────────────┼─────────────────────────────────────────────────────────────────────────────────┤                                                                                                              
  │ memory_repl_namespace_distinct_from_files      │ __repl__ does not collide with file-based programs                              │
  ├────────────────────────────────────────────────┼─────────────────────────────────────────────────────────────────────────────────┤                                                                                                              
  │ memory_cross_process_write_race                │ Two keel run subprocesses each remember 50 increments → final is 100            │
  ├────────────────────────────────────────────────┼─────────────────────────────────────────────────────────────────────────────────┤                                                                                                              
  │ memory_concurrent_reads_dont_block             │ Two processes calling only recall complete promptly                             │
  ├────────────────────────────────────────────────┼─────────────────────────────────────────────────────────────────────────────────┤                                                                                                              
  │ memory_lockfile_exists_alongside_data          │ After write, both .json and .lock present                                       │
  ├────────────────────────────────────────────────┼─────────────────────────────────────────────────────────────────────────────────┤                                                                                                              
  │ memory_corrupt_file_renamed_to_bak             │ Existing test — verify still works under lock                                   │
  ├────────────────────────────────────────────────┼─────────────────────────────────────────────────────────────────────────────────┤                                                                                                              
  │ memory_invalid_agent_name_rejected             │ Hard validation fires (synthesized via internal API or future external surface) │
  └────────────────────────────────────────────────┴─────────────────────────────────────────────────────────────────────────────────┘                                                                                                              
                                                                                                        
  Cross-process race test is the new safety-critical one. Will use Command::new(env!("CARGO_BIN_EXE_keel")) and wait() on both, then read the final value.                                                                                          
                                                                                                        
  Files touched                                                                                                                                                                                                                                     
                                                                                                        
  ┌────────────────────────────┬─────────────────────────────────────────────────────────┬───────────────────────────┐
  │            File            │                          Lines                          │           Risk            │
  ├────────────────────────────┼─────────────────────────────────────────────────────────┼───────────────────────────┤                                                                                                                              
  │ Cargo.toml                 │ +2 deps                                                 │ Low                       │
  ├────────────────────────────┼─────────────────────────────────────────────────────────┼───────────────────────────┤                                                                                                                              
  │ src/interpreter/mod.rs     │ ~30 (rewrite of program_name derivation)                │ Low                       │
  ├────────────────────────────┼─────────────────────────────────────────────────────────┼───────────────────────────┤                                                                                                                              
  │ src/runtime/mod.rs         │ ~100 (Memory section: lock plumbing, fsync, validation) │ Medium — concurrency code │
  ├────────────────────────────┼─────────────────────────────────────────────────────────┼───────────────────────────┤                                                                                                                              
  │ tests/integration_tests.rs │ +200 (new tests, especially cross-process)              │ Low                       │
  ├────────────────────────────┼─────────────────────────────────────────────────────────┼───────────────────────────┤                                                                                                                              
  │ SPEC.md §12                │ path format + multi-process safety note (now ✓)         │ Low                       │
  ├────────────────────────────┼─────────────────────────────────────────────────────────┼───────────────────────────┤                                                                                                                              
  │ docs/src/guide/agents.md   │ path format                                             │ Low                       │
  ├────────────────────────────┼─────────────────────────────────────────────────────────┼───────────────────────────┤                                                                                                                              
  │ CHANGELOG.md               │ v0.1.11 entry with break note                           │ Low                       │
  ├────────────────────────────┼─────────────────────────────────────────────────────────┼───────────────────────────┤                                                                                                                              
  │ ROADMAP.md                 │ new release row                                         │ Low                       │
  ├────────────────────────────┼─────────────────────────────────────────────────────────┼───────────────────────────┤                                                                                                                              
  │ Cargo.toml version         │ 0.1.10 → 0.1.11                                         │ Low                       │
  └────────────────────────────┴─────────────────────────────────────────────────────────┴───────────────────────────┘                                                                                                                              
                                                                                                        
  What I'm explicitly NOT doing in this release                                                                                                                                                                                                     
                                                                                                        
  - Global-context Memory — defer until top-level attribute syntax is settled                                                                                                                                                                       
  - Per-agent in-process mutexes — current behavior is correct; flock subsumes it                       
  - keel memory CLI — discoverability tooling for v0.2                                                                                                                                                                                              
  - Auto-migration from v0.1.10 layout — alpha, document break                                                                                                                                                                                      
  - fsync-on-every-recall — read path doesn't dirty state, no need                                                                                                                                                                                  
  - CRC/checksum on JSON file — corrupt-file detection via serde_json parse failure is sufficient                                                                                                                                                   
                                                                                                                                                                                                                                                    
  Open question for you                                                                                                                                                                                                                             
                                                                                                                                                                                                                                                    
  The fs2 crate vs hand-rolled flock wrapper. fs2 is ~30 KB total impact and saves writing platform-conditional unsafe code. My recommendation is fs2, but if you'd rather keep zero new I/O deps I can vendor a small wrapper. Default: use fs2    
  unless you say otherwise.
