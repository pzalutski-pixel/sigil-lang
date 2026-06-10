//! Sigil Compiler - Native code generation via LLVM
//!
//! This compiler takes .beh behavior files and produces native executables.
//! Also supports library compilation with the --lib flag.
//!
//! Features incremental compilation (compiled behaviors are cached for fast
//! rebuilds) and library compilation/linking.

use inkwell::context::Context;
use inkwell::targets::{InitializationConfig, Target, TargetMachine, RelocMode, CodeModel};
use inkwell::OptimizationLevel;
use std::path::{Path, PathBuf};
use std::fs;
use std::collections::{HashSet, HashMap};

use error::CompilerError;

mod ast;
mod cache;
mod cfg;
mod codegen;
mod diagnostic;
mod error;
mod graph;
mod hash;
mod lexer;
mod linker;
mod parser;
mod render;
mod semantic;
mod source;

// =============================================================================
// Command-line Arguments
// =============================================================================

/// Command-line arguments
struct Args {
    input: PathBuf,
    output: Option<String>,
    lib_mode: bool,
    fix_hashes: bool,             // Fix incorrect hashes in source files
    hash_only: bool,              // Compute and print one behavior's contract hash, then exit
    link_libs: Vec<PathBuf>,      // Sigil libraries (parsed for symbols AND linked)
    native_libs: Vec<PathBuf>,    // Native C libraries (passed directly to linker)
}

/// Parse command-line arguments
fn parse_args() -> Result<Args, CompilerError> {
    let args: Vec<String> = std::env::args().collect();
    let mut input = None;
    let mut output = None;
    let mut lib_mode = false;
    let mut fix_hashes = false;
    let mut hash_only = false;
    let mut link_libs = Vec::new();
    let mut native_libs = Vec::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--lib" => lib_mode = true,
            "--fix-hashes" => fix_hashes = true,
            "--hash" => hash_only = true,
            "--link" => {
                i += 1;
                if i < args.len() {
                    link_libs.push(PathBuf::from(&args[i]));
                } else {
                    return Err(CompilerError::cli("--link requires a library path"));
                }
            }
            "--native-lib" => {
                i += 1;
                if i < args.len() {
                    native_libs.push(PathBuf::from(&args[i]));
                } else {
                    return Err(CompilerError::cli("--native-lib requires a library path"));
                }
            }
            "-o" => {
                i += 1;
                if i < args.len() {
                    output = Some(args[i].clone());
                } else {
                    return Err(CompilerError::cli("-o requires an output path"));
                }
            }
            "--help" | "-h" => {
                return Err(CompilerError::cli("Usage: sigil [--lib] [--fix-hashes] [--hash] <input> [-o output] [--link lib] [--native-lib lib]..."));
            }
            arg if !arg.starts_with('-') => {
                if input.is_none() {
                    input = Some(PathBuf::from(arg));
                }
            }
            _ => return Err(CompilerError::cli(format!("Unknown argument: {}", args[i]))),
        }
        i += 1;
    }

    Ok(Args {
        input: input.ok_or_else(|| CompilerError::cli("No input file specified"))?,
        output,
        lib_mode,
        fix_hashes,
        hash_only,
        link_libs,
        native_libs,
    })
}

// =============================================================================
// File Discovery
// =============================================================================

/// Recursively search a directory for a file with the given name and extension
fn find_file_recursive(dir: &Path, name: &str, extension: &str) -> Option<PathBuf> {
    let filename = format!("{}.{}", name, extension);

    if !dir.is_dir() {
        return None;
    }

    // First check direct file in this directory
    let direct = dir.join(&filename);
    if direct.exists() {
        return Some(direct);
    }

    // Then search subdirectories
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(found) = find_file_recursive(&path, name, extension) {
                    return Some(found);
                }
            }
        }
    }

    None
}

/// Recursively search a directory for a .beh file with the given name
fn find_beh_recursive(dir: &Path, name: &str) -> Option<PathBuf> {
    find_file_recursive(dir, name, "beh")
}

/// Recursively search a directory for a .pattern file with the given name
/// Per SIGIL-LANGUAGE-REFERENCE Section 13: pattern files use .pattern extension
fn find_pattern_recursive(dir: &Path, name: &str) -> Option<PathBuf> {
    find_file_recursive(dir, name, "pattern")
}

/// Find a behavior file given its name (e.g., "println", "zero")
fn find_behavior(name: &str, search_paths: &[PathBuf]) -> Option<PathBuf> {
    let filename = format!("{}.beh", name);

    for base in search_paths {
        // Try: base/units/ recursively (for stdlib)
        let units_path = base.join("units");
        if units_path.is_dir() {
            if let Some(found) = find_beh_recursive(&units_path, name) {
                return Some(found);
            }
        }

        // Try: base/behaviors/ recursively (for project)
        let behaviors_path = base.join("behaviors");
        if behaviors_path.is_dir() {
            if let Some(found) = find_beh_recursive(&behaviors_path, name) {
                return Some(found);
            }
        }

        // Try direct: base/name.beh
        let direct_path = base.join(&filename);
        if direct_path.exists() {
            return Some(direct_path);
        }
    }

    // Content fallback: a behavior may live in a `.beh` file NOT named after it
    // (resolution used to be filename-only, which silently failed when a file
    // held a differently-named behavior). Scan for the file that actually
    // declares `BEHAVIOR <name>`. Only runs when the fast filename match fails.
    for base in search_paths {
        if let Some(found) = find_behavior_by_content(base, name) {
            return Some(found);
        }
    }
    None
}

/// Fallback resolver: recursively scan `dir` for a `.beh` file that declares
/// `BEHAVIOR <name>`, so a behavior need not live in a file named after it.
fn find_behavior_by_content(dir: &Path, name: &str) -> Option<PathBuf> {
    if !dir.is_dir() {
        return None;
    }
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(found) = find_behavior_by_content(&path, name) {
                    return Some(found);
                }
            } else if path.extension().and_then(|e| e.to_str()) == Some("beh") {
                if let Ok(src) = fs::read_to_string(&path) {
                    if src.lines().any(|l| {
                        l.trim()
                            .strip_prefix("BEHAVIOR ")
                            .map_or(false, |r| r.trim() == name)
                    }) {
                        return Some(path);
                    }
                }
            }
        }
    }
    None
}

/// Parse a behavior file (no semantic analysis yet)
/// Returns the behavior and the source content for error reporting
/// If fix_hashes is true, fixes incorrect hashes in the source file
fn parse_behavior(path: &Path, fix_hashes: bool) -> Result<(ast::Behavior, String), CompilerError> {
    let source = fs::read_to_string(path)
        .map_err(|e| CompilerError::io(format!("Error reading {}: {}", path.display(), e)))?;

    let tokens = lexer::lex(&source)?;
    let behavior = parser::parse(&tokens, &source)?;

    // Check and optionally fix hash
    if !behavior.hash.is_empty() {
        let computed = hash::compute_contract_hash(&behavior.contract);
        if behavior.hash != computed {
            if fix_hashes {
                println!("  [Fixing hash] {}: {} -> {}", behavior.name, behavior.hash, computed);
                let new_source = rewrite_hash_line(&source, &computed);
                fs::write(path, &new_source)
                    .map_err(|e| CompilerError::io(format!("Error writing {}: {}", path.display(), e)))?;

                // Re-parse with fixed hash
                let tokens = lexer::lex(&new_source)?;
                let behavior = parser::parse(&tokens, &new_source)?;
                return Ok((behavior, new_source));
            }
            // If not fixing, semantic analyzer will catch this error later
        }
    }

    Ok((behavior, source))
}

/// Rewrite the value on the `HASH` line in place, preserving the rest of the
/// file (including line endings). Locates `HASH ` at the start of a line — as
/// behavior files write it — and replaces from there to end of line, so the
/// existing value is replaced wholesale. This handles any placeholder,
/// including `00000000` (which the lexer reads as the integer 0, so a
/// substring replace on the parsed value would mangle it).
fn rewrite_hash_line(source: &str, computed: &str) -> String {
    const NEEDLE: &str = "HASH ";
    let mut search = 0;
    while let Some(rel) = source[search..].find(NEEDLE) {
        let idx = search + rel;
        let at_line_start = idx == 0 || source.as_bytes()[idx - 1] == b'\n';
        if at_line_start {
            let val_start = idx + NEEDLE.len();
            let line_end = source[val_start..]
                .find(|c: char| c == '\n' || c == '\r')
                .map(|e| val_start + e)
                .unwrap_or(source.len());
            return format!("{}{}{}", &source[..val_start], computed, &source[line_end..]);
        }
        search = idx + NEEDLE.len();
    }
    source.to_string()
}

/// Find a pattern file given its name
/// Per SIGIL-LANGUAGE-REFERENCE Section 13: pattern files use .pattern extension
fn find_pattern(name: &str, search_paths: &[PathBuf]) -> Option<PathBuf> {
    for base in search_paths {
        // Try: base/patterns/ recursively
        let patterns_path = base.join("patterns");
        if patterns_path.is_dir() {
            if let Some(found) = find_pattern_recursive(&patterns_path, name) {
                return Some(found);
            }
        }

        // Try direct: base/name.pattern
        let direct_path = base.join(format!("{}.pattern", name));
        if direct_path.exists() {
            return Some(direct_path);
        }
    }
    None
}

/// Parse a pattern file
fn parse_pattern_file(path: &Path) -> Result<ast::Pattern, CompilerError> {
    let source = fs::read_to_string(path)
        .map_err(|e| CompilerError::io(format!("Error reading {}: {}", path.display(), e)))?;

    let tokens = lexer::lex(&source)?;
    let pattern = parser::parse_pattern(&tokens, &source)?;

    Ok(pattern)
}

/// Parse an executable file (Section 11.2)
/// Returns the executable and the source content for error reporting
fn parse_executable_file(path: &Path) -> Result<(ast::Executable, String), CompilerError> {
    let source = fs::read_to_string(path)
        .map_err(|e| CompilerError::io(format!("Error reading {}: {}", path.display(), e)))?;

    let tokens = lexer::lex(&source)?;
    let executable = parser::parse_executable(&tokens, &source)?;

    Ok((executable, source))
}

// =============================================================================
// Validation
// =============================================================================

/// Validate a behavior with its dependency contracts
/// Per Section 14.3: Input match requires knowing callee contracts
/// Per Section 14.5: SPAWN requires pattern, not behavior
fn validate_behavior(
    behavior: &ast::Behavior,
    path: &Path,
    source_content: &str,
    dependency_contracts: &std::collections::HashMap<String, ast::Contract>,
    pattern_names: &HashSet<String>,
) -> Result<(), CompilerError> {
    let mut analyzer = semantic::SemanticAnalyzer::new();
    if let Err(errors) = analyzer.analyze(behavior, dependency_contracts, pattern_names) {
        // Create source manager for rich error rendering
        let mut source_mgr = source::SourceManager::new();
        source_mgr.add_source(path.to_path_buf(), source_content.to_string());

        // Convert errors to diagnostics and render
        let renderer = render::DiagnosticRenderer::new(&source_mgr);
        let mut error_output = String::new();

        for err in &errors {
            let diag = err.to_diagnostic().with_file(path);
            error_output.push_str(&renderer.render(&diag));
            error_output.push('\n');
        }

        return Err(CompilerError::semantic(None, error_output));
    }

    // Print warnings (non-fatal)
    for warning in analyzer.warnings() {
        eprintln!("Warning in {}: Line {}: {}", path.display(), warning.line, warning.message);
    }

    Ok(())
}

// =============================================================================
// Dependency Loading
// =============================================================================

/// Recursively load all behavior dependencies (parse only, no validation yet)
/// Returns Vec of (full_name, path, behavior, source) tuples
///
/// Per Section 14.3 Contract Validity:
/// - Dependencies must exist (MissingDependency error)
/// - Dependencies must have matching hash (hash verification)
///
/// If a behavior is in library_symbols, skip loading from source
fn load_dependencies(
    behavior: &ast::Behavior,
    search_paths: &[PathBuf],
    loaded: &mut HashSet<String>,
    behaviors: &mut Vec<(String, PathBuf, ast::Behavior, String)>,
    library_symbols: &HashSet<String>,
    fix_hashes: bool,
) -> Result<(), CompilerError> {
    for req in &behavior.contract.requires.behaviors {
        let name = &req.name;
        if loaded.contains(name) {
            continue;
        }

        // Skip if behavior is in a linked library
        if library_symbols.contains(name) {
            println!("  [Library] {} (from linked library)", name);
            loaded.insert(name.clone());
            continue;
        }

        let path = find_behavior(name, search_paths)
            .ok_or_else(|| CompilerError::load(format!("Could not find behavior: {}", name)))?;

        println!("  Loading dependency: {} -> {}", name, path.display());
        let (dep, source) = parse_behavior(&path, fix_hashes)?;

        // Section 14.3: Verify dependency hash matches expected
        // req.hash is what the REQUIRES clause expects (programmer intent)
        // dep.hash is what the loaded behavior declares (actual contract)
        // Mismatch means wrong version - this is an error, not auto-fixable
        if !req.hash.is_empty() && !dep.hash.is_empty() && req.hash != dep.hash {
            return Err(CompilerError::semantic(
                None,
                format!(
                    "dependency '{}' hash mismatch: REQUIRES expects {}, but found {}. \
                     Update the REQUIRES clause to use the correct hash.",
                    name, req.hash, dep.hash
                )
            ));
        }

        // Recursively load this behavior's dependencies
        load_dependencies(&dep, search_paths, loaded, behaviors, library_symbols, fix_hashes)?;

        loaded.insert(name.clone());
        behaviors.push((name.clone(), path, dep, source));
    }
    Ok(())
}

// =============================================================================
// Utilities
// =============================================================================

/// Collect SPAWN pattern references from AST nodes
fn collect_spawn_patterns(nodes: &[ast::Node], pattern_names: &mut Vec<String>) {
    for node in nodes {
        match &node.kind {
            ast::NodeKind::Assignment { expr, .. } => {
                if let ast::Expr::Spawn { pattern, .. } = expr.as_ref() {
                    if !pattern_names.contains(pattern) {
                        pattern_names.push(pattern.clone());
                    }
                }
            }
            ast::NodeKind::Scope { nodes } => {
                collect_spawn_patterns(nodes, pattern_names);
            }
            _ => {}
        }
    }
}

/// Collect CALL behavior references from ENTRY nodes
/// Returns behavior names that need to be loaded as dependencies
fn collect_call_behaviors(nodes: &[ast::Node], behavior_names: &mut Vec<String>) {
    for node in nodes {
        match &node.kind {
            ast::NodeKind::Call { behavior, .. } => {
                if !behavior_names.contains(behavior) {
                    behavior_names.push(behavior.clone());
                }
            }
            ast::NodeKind::Assignment { expr, .. } => {
                if let ast::Expr::Call { behavior, .. } = expr.as_ref() {
                    if !behavior_names.contains(behavior) {
                        behavior_names.push(behavior.clone());
                    }
                }
            }
            _ => {}
        }
    }
}

/// Load a behavior by name and recursively load its dependencies
/// For use with EXECUTABLE: loads behaviors referenced by CALL in ENTRY
fn load_behavior_by_name(
    name: &str,
    search_paths: &[PathBuf],
    loaded: &mut HashSet<String>,
    behaviors: &mut Vec<(String, PathBuf, ast::Behavior, String)>,
    library_symbols: &HashSet<String>,
    fix_hashes: bool,
) -> Result<(), CompilerError> {
    if loaded.contains(name) {
        return Ok(());
    }

    // Skip if behavior is in a linked library
    if library_symbols.contains(name) {
        println!("  [Library] {} (from linked library)", name);
        loaded.insert(name.to_string());
        return Ok(());
    }

    let path = find_behavior(name, search_paths)
        .ok_or_else(|| CompilerError::load(format!("Could not find behavior: {}", name)))?;

    println!("  Loading behavior: {} -> {}", name, path.display());
    let (dep, source) = parse_behavior(&path, fix_hashes)?;

    // Recursively load this behavior's dependencies
    load_dependencies(&dep, search_paths, loaded, behaviors, library_symbols, fix_hashes)?;

    loaded.insert(name.to_string());
    behaviors.push((name.to_string(), path, dep, source));
    Ok(())
}

// =============================================================================
// Library Utilities
// =============================================================================

/// Recursively find all .beh files in a directory
fn find_all_beh_files(dir: &Path) -> Result<Vec<PathBuf>, CompilerError> {
    let mut files = Vec::new();
    find_beh_files_in_dir(dir, &mut files)?;
    files.sort();
    Ok(files)
}

fn find_beh_files_in_dir(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), CompilerError> {
    if !dir.is_dir() {
        return Ok(());
    }

    let entries = fs::read_dir(dir)
        .map_err(|e| CompilerError::io(format!("Failed to read directory {}: {}", dir.display(), e)))?;

    for entry in entries {
        let entry = entry.map_err(|e| CompilerError::io(format!("Failed to read entry: {}", e)))?;
        let path = entry.path();

        if path.is_dir() {
            find_beh_files_in_dir(&path, files)?;
        } else if path.extension().map(|e| e == "beh").unwrap_or(false) {
            files.push(path);
        }
    }

    Ok(())
}

/// Generate .generated/behaviors.index and contracts.registry for a library
fn generate_library_index(behaviors: &[(String, ast::Behavior)], output_dir: &Path) -> Result<(), CompilerError> {
    let gen_dir = output_dir.join(".generated");
    fs::create_dir_all(&gen_dir)
        .map_err(|e| CompilerError::io(format!("Failed to create .generated dir: {}", e)))?;

    let mut index = String::new();
    for (name, behavior) in behaviors {
        index.push_str(&format!("{}@{}\n", name, behavior.hash));
    }
    fs::write(gen_dir.join("behaviors.index"), &index)
        .map_err(|e| CompilerError::io(format!("Failed to write behaviors.index: {}", e)))?;

    let mut registry = String::new();
    for (_name, behavior) in behaviors {
        registry.push_str(&format!("BEHAVIOR {}\n", behavior.name));

        if let Some(desc) = &behavior.description {
            registry.push_str("DESCRIPTION\n");
            for line in desc.lines() {
                registry.push_str(&format!("  {}\n", line));
            }
        }

        registry.push_str("CONTRACT\n");

        for input in &behavior.contract.inputs {
            let type_str = format!("{:?}", input.typ).to_lowercase();
            registry.push_str(&format!("  INPUT {} {} {}\n", input.name, type_str, input.size));
        }

        for output in &behavior.contract.outputs {
            let type_str = format!("{:?}", output.typ).to_lowercase();
            registry.push_str(&format!("  OUTPUT {} {} {}\n", output.name, type_str, output.size));
        }

        if !behavior.contract.requires.behaviors.is_empty() {
            registry.push_str("  REQUIRES");
            for req in &behavior.contract.requires.behaviors {
                registry.push_str(&format!(" {}@{}", req.name, req.hash));
            }
            registry.push('\n');
        }

        if !behavior.contract.guarantees.is_empty() {
            registry.push_str("  GUARANTEES");
            for g in &behavior.contract.guarantees {
                let g_str = match g {
                    ast::Guarantee::Pure => "pure",
                    ast::Guarantee::NoAlloc => "no_alloc",
                    ast::Guarantee::WritesOutput => "writes_output",
                };
                registry.push_str(&format!(" {}", g_str));
            }
            registry.push('\n');
        }

        registry.push_str(&format!("HASH {}\n", behavior.hash));
        registry.push_str("END\n\n");
    }

    fs::write(gen_dir.join("contracts.registry"), &registry)
        .map_err(|e| CompilerError::io(format!("Failed to write contracts.registry: {}", e)))?;

    // Scannable catalog: one line per behavior (name, signature, first sentence of
    // its description). The full contracts.registry is verbose; this lets a reader
    // (or an AI author) see at a glance what already exists, instead of
    // reimplementing a helper that's already in the library.
    let mut catalog = String::from(
        "# Standard Library Catalog\n\n\
         One line per behavior — name, (inputs → outputs), and purpose. \
         Full contracts (sizes, hashes, guarantees) are in `contracts.registry`.\n\n");
    let mut sorted: Vec<&(String, ast::Behavior)> = behaviors.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, behavior) in sorted {
        let ins: Vec<&str> = behavior.contract.inputs.iter().map(|p| p.name.as_str()).collect();
        let outs: Vec<&str> = behavior.contract.outputs.iter().map(|p| p.name.as_str()).collect();
        let sig = format!(
            "{} → {}",
            if ins.is_empty() { "()".to_string() } else { ins.join(", ") },
            if outs.is_empty() { "()".to_string() } else { outs.join(", ") },
        );
        // First sentence of the DESCRIPTION. Source lines wrap mid-sentence, so a
        // physical line is not a usable summary; an "e.g."/"i.e." period does not
        // end a sentence.
        let purpose = behavior.description.as_deref()
            .map(|d| {
                let flat = d.split_whitespace().collect::<Vec<_>>().join(" ");
                let mut end = flat.len();
                let mut from = 0;
                while let Some(pos) = flat[from..].find(". ") {
                    let dot = from + pos;
                    if flat[..=dot].ends_with("e.g.") || flat[..=dot].ends_with("i.e.") {
                        from = dot + 1;
                    } else {
                        end = dot + 1;
                        break;
                    }
                }
                flat[..end].to_string()
            })
            .unwrap_or_default();
        catalog.push_str(&format!("- `{}` ({}) — {}\n", name, sig, purpose));
    }
    fs::write(gen_dir.join("catalog.md"), &catalog)
        .map_err(|e| CompilerError::io(format!("Failed to write catalog.md: {}", e)))?;

    println!("Generated .generated/behaviors.index, contracts.registry, and catalog.md");
    Ok(())
}

/// Load library symbols from .generated/behaviors.index
fn load_library_symbols(lib_path: &Path) -> Result<HashSet<String>, CompilerError> {
    let index_dir = lib_path.parent().unwrap_or(Path::new(".")).join(".generated");
    let index_path = index_dir.join("behaviors.index");

    let mut behaviors = HashSet::new();
    if index_path.exists() {
        let content = fs::read_to_string(&index_path)
            .map_err(|e| CompilerError::io(format!("Failed to read index: {}", e)))?;
        for line in content.lines() {
            if let Some(name) = line.split('@').next() {
                let name = name.trim();
                if !name.is_empty() {
                    behaviors.insert(name.to_string());
                }
            }
        }
    }
    Ok(behaviors)
}

/// Load library contracts from .generated/contracts.registry
fn load_library_contracts(lib_path: &Path) -> Result<HashMap<String, ast::Contract>, CompilerError> {
    let index_dir = lib_path.parent().unwrap_or(Path::new(".")).join(".generated");
    let registry_path = index_dir.join("contracts.registry");

    let mut contracts = HashMap::new();
    if registry_path.exists() {
        let content = fs::read_to_string(&registry_path)
            .map_err(|e| CompilerError::io(format!("Failed to read contracts.registry: {}", e)))?;

        let mut start = 0;
        while let Some(beh_pos) = content[start..].find("BEHAVIOR ") {
            let abs_beh = start + beh_pos;
            if let Some(end_pos) = content[abs_beh..].find("\nEND") {
                let block = &content[abs_beh..abs_beh + end_pos + 4];
                match lexer::lex(block) {
                    Ok(tokens) => {
                        match parser::parse(&tokens, block) {
                            Ok(behavior) => {
                                println!("    Loaded contract: {}", behavior.name);
                                contracts.insert(behavior.name.clone(), behavior.contract);
                            }
                            Err(e) => {
                                eprintln!("    Warning: Failed to parse contract block: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("    Warning: Failed to lex contract block: {}", e);
                    }
                }
                start = abs_beh + end_pos + 4;
            } else {
                break;
            }
        }
    }
    Ok(contracts)
}

// =============================================================================
// Main Entry Point
// =============================================================================

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}

/// Main compiler entry point
fn run() -> Result<(), CompilerError> {
    let args = parse_args()?;

    // --hash: compute and print one behavior's contract hash, then exit. This is
    // the "compute the hash for me" tool an author runs and copies the result
    // into the behavior's HASH line (and into callers' REQUIRES). It does NOT
    // modify any file, and works even when the file has no HASH line yet, because
    // the hash is computed from the CONTRACT. Output is just the 8-char hash, so
    // it can be captured directly.
    if args.hash_only {
        let (behavior, _src) = parse_behavior(&args.input, false)?;
        println!("{}", hash::compute_contract_hash(&behavior.contract));
        return Ok(());
    }

    println!("Sigil Compiler v0.1.0");

    // Initialize LLVM
    Target::initialize_native(&InitializationConfig::default())
        .map_err(|e| CompilerError::codegen(format!("Failed to initialize native target: {}", e)))?;

    // Get native target
    let triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&triple)
        .map_err(|e| CompilerError::codegen(format!("Failed to get target: {}", e)))?;
    let target_machine = target
        .create_target_machine(
            &triple,
            "generic",
            "",
            OptimizationLevel::Default,
            // PIC: Linux links executables as PIE by default (absolute relocs are
            // rejected) and macOS requires position-independent code.
            RelocMode::PIC,
            CodeModel::Default,
        )
        .ok_or_else(|| CompilerError::codegen("Failed to create target machine"))?;

    let triple_str = triple.as_str().to_str().unwrap();
    println!("Target: {}", triple_str);

    // Detect target platform from triple (Section 12)
    let target_platform = codegen::TargetPlatform::from_triple(triple_str);
    println!("Platform: {:?}", target_platform);

    // Library compilation mode: compile folder of .beh files to .lib
    if args.lib_mode {
        #[cfg(target_os = "windows")]
        let default_ext = "lib";
        #[cfg(not(target_os = "windows"))]
        let default_ext = "a";

        let input_dir = &args.input;
        if !input_dir.is_dir() {
            return Err(CompilerError::cli(format!(
                "--lib requires a directory of .beh files, got: {}",
                input_dir.display()
            )));
        }

        let output_path = args.output
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let name = input_dir.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "library".to_string());
                input_dir.join(format!("{}.{}", name, default_ext))
            });

        println!("Compiling library from: {}", input_dir.display());

        // Discover all .beh files
        let beh_files = find_all_beh_files(input_dir)?;
        if beh_files.is_empty() {
            return Err(CompilerError::cli(format!(
                "No .beh files found in {}",
                input_dir.display()
            )));
        }
        println!("Found {} behavior files", beh_files.len());

        // Parse all behaviors
        let mut behaviors: Vec<(String, PathBuf, ast::Behavior, String)> = Vec::new();
        for path in &beh_files {
            let (behavior, source) = parse_behavior(path, args.fix_hashes)?;
            println!("  [{}] {}", if behavior.is_native { "Native" } else { "Parse" }, behavior.name);
            behaviors.push((behavior.name.clone(), path.clone(), behavior, source));
        }

        // Build contracts map for validation
        let mut all_contracts: HashMap<String, ast::Contract> = HashMap::new();
        for (name, _, beh, _) in &behaviors {
            all_contracts.insert(name.clone(), beh.contract.clone());
        }

        // Validate all behaviors
        let empty_patterns = HashSet::new();
        for (_name, path, beh, source) in &behaviors {
            if !beh.is_native {
                validate_behavior(beh, path, source, &all_contracts, &empty_patterns)?;
            }
        }

        // Compile each non-native behavior to .o
        let cache_dir = output_path.parent().unwrap_or(Path::new(".")).join(".cache");
        let objects_dir = cache_dir.join("objects");
        fs::create_dir_all(&objects_dir)
            .map_err(|e| CompilerError::io(format!("Failed to create cache directory: {}", e)))?;

        let mut manifest = cache::CacheManifest::load(&cache_dir);
        let mut object_files: Vec<PathBuf> = Vec::new();

        // Reject duplicate behavior names: each behavior compiles to a global
        // `sigil_<name>` symbol, so two behaviors sharing a name put two
        // colliding objects in the archive and the linker silently picks one
        // (silent wrong code). Names must be globally unique.
        let mut seen_names: HashSet<&str> = HashSet::new();
        for (name, path, _, _) in &behaviors {
            if !seen_names.insert(name.as_str()) {
                return Err(CompilerError::semantic(None, format!(
                    "duplicate behavior name '{}' ({}): behavior names must be globally unique \
                     (two behaviors with the same name collide on the sigil_{} symbol)",
                    name, path.display(), name.replace('-', "_"))));
            }
        }

        for (name, path, beh, _) in &behaviors {
            if beh.is_native {
                continue;
            }

            let source_hash = cache::compute_source_hash(path).unwrap_or_default();
            // Use each dependency's actual current contract hash (see the executable
            // path below) so a changed linked contract invalidates the cache.
            let dep_hash_strs: Vec<(String, String)> = beh.contract.requires.behaviors.iter()
                .filter_map(|r| all_contracts.get(&r.name)
                    .map(|c| (r.name.clone(), hash::compute_contract_hash(c))))
                .collect();
            let dep_hashes: Vec<(&str, &str)> = dep_hash_strs.iter()
                .map(|(n, h)| (n.as_str(), h.as_str())).collect();
            let deps_hash = cache::compute_deps_hash(&dep_hashes);
            let obj_path = cache::object_path(&cache_dir, name, &source_hash);

            if !manifest.needs_recompile(name, &source_hash, &beh.hash, &deps_hash) {
                println!("  [Cached] {}", name);
                object_files.push(obj_path);
                continue;
            }

            println!("  [Compiling] {}", name);

            let ctx = Context::create();
            let mut codegen = codegen::CodeGen::new(&ctx, name, target_platform);

            for req in &beh.contract.requires.behaviors {
                if let Some(contract) = all_contracts.get(&req.name) {
                    codegen.declare_external_behavior(&req.name, contract);
                }
            }

            codegen.compile_as_function(beh, name)
                .map_err(|e| CompilerError::codegen(format!("Codegen error for {}: {}", name, e)))?;

            codegen.run_optimization_passes(&target_machine)
                .map_err(|e| CompilerError::codegen(format!("Optimization error for {}: {}", name, e)))?;

            codegen.write_object_file(&target_machine, &obj_path)
                .map_err(|e| CompilerError::io(format!("Failed to write object file for {}: {}", name, e)))?;

            manifest.update_entry(name, cache::CacheEntry {
                source_path: path.clone(),
                source_hash: source_hash.clone(),
                contract_hash: beh.hash.clone(),
                deps_hash,
                object_path: obj_path.clone(),
                timestamp: cache::current_timestamp(),
            });

            object_files.push(obj_path);
        }

        manifest.save(&cache_dir).ok();

        // Archive into library
        linker::archive_objects(&object_files, &output_path)?;

        // Generate index
        let lib_behaviors: Vec<(String, ast::Behavior)> = behaviors.iter()
            .map(|(n, _, b, _)| (n.clone(), b.clone()))
            .collect();
        let output_dir = output_path.parent().unwrap_or(Path::new("."));
        generate_library_index(&lib_behaviors, output_dir)?;

        println!("Successfully built: {}", output_path.display());
        return Ok(());
    }

    // Standard executable compilation mode
    let input_path = &args.input;

    // Determine output path: use -o directly if provided, otherwise output next to input file
    let output_path = if let Some(ref output) = args.output {
        PathBuf::from(output)
    } else {
        #[cfg(target_os = "windows")]
        let default_name = input_path.file_stem()
            .map(|s| format!("{}.exe", s.to_string_lossy()))
            .unwrap_or_else(|| "output.exe".to_string());
        #[cfg(not(target_os = "windows"))]
        let default_name = input_path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "output".to_string());
        let parent = input_path.parent().unwrap_or(Path::new("."));
        parent.join(default_name)
    };

    // Derive build directory from output path for cache
    let build_dir = output_path.parent().unwrap_or(Path::new("."));

    // Load library symbols and contracts from --link flags
    let mut library_symbols: HashSet<String> = HashSet::new();
    let mut library_contracts: HashMap<String, ast::Contract> = HashMap::new();
    let link_libs = args.link_libs.clone();

    for lib_path in &link_libs {
        println!("Loading library: {}", lib_path.display());
        match load_library_symbols(lib_path) {
            Ok(symbols) => {
                println!("  Found {} behaviors", symbols.len());
                library_symbols.extend(symbols);
            }
            Err(e) => {
                eprintln!("Warning: Failed to load library symbols from {}: {}", lib_path.display(), e);
            }
        }
        match load_library_contracts(lib_path) {
            Ok(contracts) => {
                for (name, contract) in contracts {
                    library_contracts.insert(name, contract);
                }
            }
            Err(e) => {
                eprintln!("Warning: Failed to load library contracts from {}: {}", lib_path.display(), e);
            }
        }
    }

    // Validate input file extension: must be .sigil for entry point
    let extension = input_path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    if extension != "sigil" {
        return Err(CompilerError::cli(format!(
            "Entry point must be a .sigil file, got: {}. \
             Use EXECUTABLE...END_EXECUTABLE in a .sigil file as entry point. \
             BEHAVIOR files (.beh) are callable units, not entry points.",
            input_path.display()
        )));
    }

    // Parse executable (Section 11.2)
    let (executable, _entry_source) = parse_executable_file(input_path)?;

    println!("Entry executable: {}", executable.name);

    // Section 6: ENTRY is a composite — it wires behaviors and must not perform
    // primitive computation. (ENTRY has no contract, so it isn't otherwise run
    // through the per-behavior analyzer; this is its dedicated role check.)
    let entry_violations = semantic::check_entry_is_composite(&executable.entry);
    if !entry_violations.is_empty() {
        let msg = entry_violations
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n  ");
        return Err(CompilerError::semantic(
            None,
            format!("ENTRY must wire behaviors, not compute:\n  {}", msg),
        ));
    }

    // Set up search paths for dependencies
    let mut search_paths = Vec::new();

    // Add project root (parent of input file's directory)
    if let Some(parent) = input_path.parent() {
        // If input is in behaviors/, go up one level
        if let Some(grandparent) = parent.parent() {
            search_paths.push(grandparent.to_path_buf());
        }
        search_paths.push(parent.to_path_buf());
    }

    // Add stdlib path (relative to working directory)
    search_paths.push(PathBuf::from("stdlib"));
    // Also try relative to input file
    if let Some(parent) = input_path.parent() {
        if let Some(grandparent) = parent.parent() {
            search_paths.push(grandparent.join("stdlib"));
            // Also go up another level for examples
            if let Some(great_grandparent) = grandparent.parent() {
                search_paths.push(great_grandparent.join("stdlib"));
            }
        }
    }

    // Phase 2: Load behaviors referenced by CALL in ENTRY
    // EXECUTABLE uses CALL to invoke behaviors (no REQUIRES like BEHAVIOR)
    let mut loaded = HashSet::new();
    let mut dependencies: Vec<(String, PathBuf, ast::Behavior, String)> = Vec::new();

    // Extract CALL targets from ENTRY block
    let mut call_targets = Vec::new();
    collect_call_behaviors(&executable.entry, &mut call_targets);

    // Load each called behavior and its dependencies
    for name in &call_targets {
        load_behavior_by_name(name, &search_paths, &mut loaded, &mut dependencies, &library_symbols, args.fix_hashes)?;
    }

    // Phase 3: Load patterns from USES declarations and SPAWN in ENTRY
    let mut patterns: Vec<(String, PathBuf, ast::Pattern)> = Vec::new();
    let mut loaded_patterns: HashSet<String> = HashSet::new();

    // USES declarations from executable (explicit pattern dependencies)
    for pattern_name in &executable.uses {
        if loaded_patterns.contains(pattern_name) {
            continue;
        }

        let path = find_pattern(pattern_name, &search_paths)
            .ok_or_else(|| CompilerError::load(format!("Could not find pattern from USES: {}", pattern_name)))?;

        println!("  Loading pattern (USES): {} -> {}", pattern_name, path.display());
        let pattern = parse_pattern_file(&path)?;
        loaded_patterns.insert(pattern_name.clone());
        patterns.push((pattern_name.clone(), path, pattern));
    }

    // Also scan ENTRY for any SPAWN (patterns not declared in USES)
    let mut spawn_patterns = Vec::new();
    collect_spawn_patterns(&executable.entry, &mut spawn_patterns);

    for pattern_name in &spawn_patterns {
        if loaded_patterns.contains(pattern_name) {
            continue;
        }

        let path = find_pattern(pattern_name, &search_paths)
            .ok_or_else(|| CompilerError::load(format!("Could not find pattern: {}", pattern_name)))?;

        println!("  Loading pattern (SPAWN): {} -> {}", pattern_name, path.display());
        let pattern = parse_pattern_file(&path)?;
        loaded_patterns.insert(pattern_name.clone());
        patterns.push((pattern_name.clone(), path, pattern));
    }

    // Phase 3.5: Load behaviors called from pattern bodies (ON_CREATE / ON_DESTROY).
    // Patterns have no REQUIRES clause, so their CALL targets must be discovered,
    // loaded, and compiled the same way ENTRY's are. Without this, a pattern that
    // calls project behaviors would fail to link.
    let mut pattern_call_targets = Vec::new();
    for (_, _, pattern) in &patterns {
        collect_call_behaviors(&pattern.on_create, &mut pattern_call_targets);
        collect_call_behaviors(&pattern.on_destroy, &mut pattern_call_targets);
    }
    for name in &pattern_call_targets {
        load_behavior_by_name(name, &search_paths, &mut loaded, &mut dependencies, &library_symbols, args.fix_hashes)?;
    }

    // Phase 4: Build dependency contracts map for validation
    // Per Section 14.3: Input match requires knowing callee contracts
    let mut dependency_contracts: std::collections::HashMap<String, ast::Contract> =
        std::collections::HashMap::new();
    for (name, _, dep, _) in &dependencies {
        dependency_contracts.insert(name.clone(), dep.contract.clone());
    }
    // Linked-library behaviors are also valid CALL targets; include their
    // contracts so argument checking and output-consumption (Section 7.8) can
    // see the callee's outputs.
    for (name, contract) in &library_contracts {
        dependency_contracts.entry(name.clone()).or_insert_with(|| contract.clone());
    }

    // Phase 5: Validate all loaded behaviors with dependency contracts
    // EXECUTABLE has no contract - only validate behaviors
    for (_name, path, dep, dep_source) in &dependencies {
        // Each dependency's deps are a subset of all loaded deps
        // Dependencies typically don't spawn, so empty pattern set is fine
        let empty_patterns = HashSet::new();
        validate_behavior(dep, path, dep_source, &dependency_contracts, &empty_patterns)?;
    }

    // Phase 5.5: Whole-graph validation (cross-behavior).
    // Builds a dependency graph from REQUIRES + guarantees and runs checks that span
    // behaviors and that the per-behavior passes cannot see: dependency resolution,
    // contract-hash consistency, circular dependencies, and TRANSITIVE PURITY (a
    // `pure` behavior must not call an impure one — the per-behavior `pure` check
    // only looks inside a single behavior).
    //
    // (The graph module also has structural gap detection — unsourced inputs /
    // unconsumed outputs — but that relies on a fully wired edge model, which the
    // V2 graph-native flow provides and V1 composite syntax only approximates, so it
    // is not run here; it would over-report on reconstructed V1 edges.)
    {
        use graph::validate::GraphErrorKind;
        let mut pg = graph::ProjectGraph::new();
        for (_n, p, dep, _) in &dependencies {
            let node = graph::builder::behavior_to_node(&mut pg, dep, Some(p.clone()));
            pg.add_node(node);
        }
        for (lname, contract) in &library_contracts {
            let h = hash::compute_contract_hash(contract);
            pg.add_library_node(graph::builder::contract_to_ref(lname, contract, &h, PathBuf::new()));
        }
        // Cross-behavior checks the per-behavior passes cannot do:
        //   - StaleDependency: a REQUIRES pin (`dep@hash`) must match the
        //     dependency's CURRENT contract hash. The per-behavior semantic pass
        //     only verifies a behavior's OWN `HASH` line, never its REQUIRES
        //     pins — and library (stdlib) deps are skipped by the loader
        //     entirely — so without this surfaced here, a stale pin to a changed
        //     dependency goes uncaught. That is the flagship "the hash IS the
        //     memory; a changed dependency is caught by hash mismatch" property,
        //     so it must be fatal (the library node's hash is computed above the
        //     same way `--hash` and the registry compute it).
        //   - UndefinedDependency: a REQUIRES name that resolves to neither a
        //     local behavior nor a linked library contract.
        //   - TransitivePurityViolation: `pure` must hold through CALL chains.
        //   - CircularDependency: REQUIRES must form a DAG.
        let mut fatal: Vec<String> = Vec::new();
        for ge in graph::validate::validate_graph(&pg) {
            if matches!(
                ge.kind,
                GraphErrorKind::StaleDependency { .. }
                    | GraphErrorKind::UndefinedDependency { .. }
                    | GraphErrorKind::TransitivePurityViolation { .. }
                    | GraphErrorKind::CircularDependency { .. }
            ) {
                fatal.push(ge.to_string());
            }
        }
        if !fatal.is_empty() {
            return Err(CompilerError::semantic(
                None,
                format!("whole-graph validation failed:\n  {}", fatal.join("\n  ")),
            ));
        }
    }

    // Note: EXECUTABLE has no CONTRACT - no validation needed for entry point

    // Phase 6: Per-behavior incremental compilation
    // Each behavior compiles to its own object file, cached by source hash
    let cache_dir = build_dir.join(".cache");
    let objects_dir = cache_dir.join("objects");
    fs::create_dir_all(&objects_dir)
        .map_err(|e| CompilerError::io(format!("Failed to create cache/objects directory: {}", e)))?;

    let mut manifest = cache::CacheManifest::load(&cache_dir);
    let mut object_files: Vec<PathBuf> = Vec::new();

    // Build map of all behavior contracts for external declarations
    // EXECUTABLE has no contract - only use dependency contracts
    let mut all_contracts: std::collections::HashMap<String, ast::Contract> = dependency_contracts.clone();

    // Add library contracts for external declarations
    for (name, contract) in &library_contracts {
        all_contracts.insert(name.clone(), contract.clone());
    }

    // Compile each dependency to its own object file
    for (full_name, path, dep, _) in &dependencies {
        let source_hash = cache::compute_source_hash(path).unwrap_or_default();

        // Compute deps hash from the ACTUAL current contract hash of each direct
        // dependency (not the as-written REQUIRES hash), so a changed dependency —
        // including a rebuilt linked-library contract whose REQUIRES text didn't
        // move — invalidates this object instead of silently reusing a stale one.
        let dep_hash_strs: Vec<(String, String)> = dep.contract.requires.behaviors.iter()
            .filter_map(|r| all_contracts.get(&r.name)
                .map(|c| (r.name.clone(), hash::compute_contract_hash(c))))
            .collect();
        let dep_hashes: Vec<(&str, &str)> = dep_hash_strs.iter()
            .map(|(n, h)| (n.as_str(), h.as_str())).collect();
        let deps_hash = cache::compute_deps_hash(&dep_hashes);

        let obj_path = cache::object_path(&cache_dir, full_name, &source_hash);

        if !manifest.needs_recompile(full_name, &source_hash, &dep.hash, &deps_hash) {
            println!("[Cached] {}", full_name);
            object_files.push(obj_path);
            continue;
        }

        println!("[Compiling] {}", full_name);

        // Create fresh CodeGen for this behavior
        let dep_context = Context::create();
        let mut dep_codegen = codegen::CodeGen::new(&dep_context, full_name, target_platform);

        // Declare external functions for all other behaviors this one might call
        for req in &dep.contract.requires.behaviors {
            if let Some(contract) = all_contracts.get(&req.name) {
                dep_codegen.declare_external_behavior(&req.name, contract);
            }
        }

        // Compile the behavior
        dep_codegen.compile_as_function(dep, full_name)
            .map_err(|e| CompilerError::codegen(format!("Codegen error for {}: {}", full_name, e)))?;

        // Run optimization passes
        dep_codegen.run_optimization_passes(&target_machine)
            .map_err(|e| CompilerError::codegen(format!("Optimization error for {}: {}", full_name, e)))?;

        // Write object file
        dep_codegen.write_object_file(&target_machine, &obj_path)
            .map_err(|e| CompilerError::io(format!("Failed to write object file for {}: {}", full_name, e)))?;

        // Update cache
        manifest.update_entry(full_name, cache::CacheEntry {
            source_path: path.clone(),
            source_hash: source_hash.clone(),
            contract_hash: dep.hash.clone(),
            deps_hash,
            object_path: obj_path.clone(),
            timestamp: cache::current_timestamp(),
        });

        object_files.push(obj_path);
    }

    // Compile each pattern to its own object file
    for (name, path, pattern) in &patterns {
        let source_hash = cache::compute_source_hash(path).unwrap_or_default();
        let deps_hash = "none".to_string(); // Patterns don't have REQUIRES

        let obj_path = cache::object_path(&cache_dir, name, &source_hash);

        if !manifest.needs_recompile(name, &source_hash, "", &deps_hash) {
            println!("[Cached] pattern {}", name);
            object_files.push(obj_path);
            continue;
        }

        println!("[Compiling] pattern {}", name);

        // Create fresh CodeGen for this pattern
        let pattern_context = Context::create();
        let mut pattern_codegen = codegen::CodeGen::new(&pattern_context, name, target_platform);

        // Declare external behaviors that patterns may call
        for (beh_name, contract) in &all_contracts {
            pattern_codegen.declare_external_behavior(beh_name, contract);
        }

        // Compile the pattern
        pattern_codegen.compile_pattern_as_function(pattern)
            .map_err(|e| CompilerError::codegen(format!("Codegen error for pattern {}: {}", name, e)))?;

        // Run optimization passes
        pattern_codegen.run_optimization_passes(&target_machine)
            .map_err(|e| CompilerError::codegen(format!("Optimization error for pattern {}: {}", name, e)))?;

        // Write object file
        pattern_codegen.write_object_file(&target_machine, &obj_path)
            .map_err(|e| CompilerError::io(format!("Failed to write object file for pattern {}: {}", name, e)))?;

        // Update cache
        manifest.update_entry(name, cache::CacheEntry {
            source_path: path.clone(),
            source_hash: source_hash.clone(),
            contract_hash: String::new(),
            deps_hash,
            object_path: obj_path.clone(),
            timestamp: cache::current_timestamp(),
        });

        object_files.push(obj_path);
    }

    // Compile entry executable (always recompile for now - it's main)
    // EXECUTABLE has no REQUIRES - dependencies from CALL statements
    let entry_hash = cache::compute_source_hash(input_path).unwrap_or_default();
    // Get hashes from loaded behaviors (not contracts - contracts don't store hash)
    let entry_deps: Vec<(&str, &str)> = dependencies.iter()
        .map(|(name, _, beh, _)| (name.as_str(), beh.hash.as_str()))
        .collect();
    let entry_deps_hash = cache::compute_deps_hash(&entry_deps);
    let entry_obj_path = cache::object_path(&cache_dir, &executable.name, &entry_hash);

    let entry_needs_rebuild = manifest.needs_recompile(
        &executable.name, &entry_hash, "", &entry_deps_hash  // No contract hash for EXECUTABLE
    );

    if entry_needs_rebuild {
        println!("[Compiling] entry {}", executable.name);

        // Create fresh CodeGen for entry
        let entry_context = Context::create();
        let mut entry_codegen = codegen::CodeGen::new(&entry_context, "main", target_platform);

        // Declare external functions for dependencies
        for (dep_name, _, dep, _) in &dependencies {
            entry_codegen.declare_external_behavior(dep_name, &dep.contract);
        }

        // Declare external functions for library behaviors (from --link)
        for (lib_name, contract) in &library_contracts {
            entry_codegen.declare_external_behavior(lib_name, contract);
        }

        // Declare external functions for patterns (compiled to separate .o files)
        for (pattern_name, _, pattern) in &patterns {
            entry_codegen.declare_external_pattern(pattern_name, pattern);
        }

        // Compile executable as main (Section 11.2)
        entry_codegen.compile_as_executable(&executable)
            .map_err(|e| CompilerError::codegen(format!("Codegen error: {}", e)))?;

        // Run optimization passes
        entry_codegen.run_optimization_passes(&target_machine)
            .map_err(|e| CompilerError::codegen(format!("Optimization error: {}", e)))?;

        // Write object file
        entry_codegen.write_object_file(&target_machine, &entry_obj_path)
            .map_err(|e| CompilerError::io(format!("Failed to write entry object file: {}", e)))?;

        // Update cache
        manifest.update_entry(&executable.name, cache::CacheEntry {
            source_path: input_path.to_path_buf(),
            source_hash: entry_hash,
            contract_hash: String::new(),  // EXECUTABLE has no contract hash
            deps_hash: entry_deps_hash,
            object_path: entry_obj_path.clone(),
            timestamp: cache::current_timestamp(),
        });
    } else {
        println!("[Cached] entry {}", executable.name);
    }

    object_files.push(entry_obj_path);

    // Save cache manifest
    if let Err(e) = manifest.save(&cache_dir) {
        eprintln!("Warning: Failed to save cache manifest: {}", e);
    }

    // Link all object files
    println!("\nLinking {} object files...", object_files.len());
    if !link_libs.is_empty() {
        println!("Including {} libraries", link_libs.len());
    }
    linker::link_objects(&object_files, &link_libs, &args.native_libs, &output_path)?;

    Ok(())
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rewrite_hash_line_zero_placeholder() {
        // Regression: `HASH 00000000` lexes as the integer 0, so the old
        // substring replace on the parsed value mangled it. The line must be
        // replaced wholesale.
        let src = "BEHAVIOR x\n\nCONTRACT\n  OUTPUT s int 8\n\nHASH 00000000\n\nIMPLEMENTATION\n  SET s 0\nEND\n";
        let out = rewrite_hash_line(src, "a222ae31");
        assert!(out.contains("\nHASH a222ae31\n"), "HASH not replaced cleanly: {}", out);
        assert!(!out.contains("0000000"), "left mangled zeros: {}", out);
        // Everything else preserved.
        assert!(out.contains("BEHAVIOR x") && out.contains("SET s 0"));
    }

    #[test]
    fn test_rewrite_hash_line_preserves_other_content() {
        let src = "HASH deadbeef\nREQUIRES foo@deadbeef\n";
        let out = rewrite_hash_line(src, "12ab34cd");
        // Only the HASH line changes; a REQUIRES pin that happens to share the
        // placeholder is untouched.
        assert!(out.starts_with("HASH 12ab34cd\n"));
        assert!(out.contains("REQUIRES foo@deadbeef"));
    }

    #[test]
    fn test_collect_spawn_patterns_empty() {
        let nodes = vec![];
        let mut patterns = vec![];
        collect_spawn_patterns(&nodes, &mut patterns);
        assert!(patterns.is_empty());
    }

    #[test]
    fn test_collect_spawn_patterns_finds_spawn() {
        use ast::{Node, NodeKind, Expr};
        use lexer::Span;

        let nodes = vec![
            Node {
                kind: NodeKind::Assignment {
                    target: "task".to_string(),
                    expr: Box::new(Expr::Spawn {
                        pattern: "worker".to_string(),
                        args: vec![],
                    }),
                },
                span: Span { line: 1, col: 1, len: 10 },
            }
        ];

        let mut patterns = vec![];
        collect_spawn_patterns(&nodes, &mut patterns);
        assert_eq!(patterns, vec!["worker".to_string()]);
    }

    #[test]
    fn test_collect_spawn_patterns_no_duplicates() {
        use ast::{Node, NodeKind, Expr};
        use lexer::Span;

        let nodes = vec![
            Node {
                kind: NodeKind::Assignment {
                    target: "task1".to_string(),
                    expr: Box::new(Expr::Spawn {
                        pattern: "worker".to_string(),
                        args: vec![],
                    }),
                },
                span: Span { line: 1, col: 1, len: 10 },
            },
            Node {
                kind: NodeKind::Assignment {
                    target: "task2".to_string(),
                    expr: Box::new(Expr::Spawn {
                        pattern: "worker".to_string(),
                        args: vec![],
                    }),
                },
                span: Span { line: 2, col: 1, len: 10 },
            }
        ];

        let mut patterns = vec![];
        collect_spawn_patterns(&nodes, &mut patterns);
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0], "worker");
    }
}
