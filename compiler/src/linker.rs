//! Linker Module
//!
//! Platform-specific linking of object files into executables.
//! Also handles archiving object files into static libraries.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;

use crate::error::CompilerError;

/// Archive multiple object files into a static library using llvm-ar
pub fn archive_objects(obj_paths: &[PathBuf], lib_path: &Path) -> Result<(), CompilerError> {
    if obj_paths.is_empty() {
        return Err(CompilerError::link("No object files to archive"));
    }

    if let Some(parent) = lib_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| CompilerError::io(format!("Failed to create output directory: {}", e)))?;
    }

    // Rebuild the archive from exactly the objects passed this build. `llvm-ar
    // rcs` REPLACES/INSERTS members but never REMOVES ones absent from the
    // argument list, so updating an existing archive after a behavior's source
    // changed (its object is renamed by content hash) would leave the old
    // `<name>_<oldhash>.o` member behind — and the linker could resolve the
    // stale symbol. Removing the archive first makes rcs create it fresh,
    // containing only the current object set. (Stale `.o` files in the cache
    // dir are then harmless: they are never passed here.)
    if lib_path.exists() {
        fs::remove_file(lib_path)
            .map_err(|e| CompilerError::io(format!("Failed to remove stale archive {}: {}", lib_path.display(), e)))?;
    }

    let mut args = vec!["rcs".to_string(), lib_path.to_string_lossy().to_string()];
    for obj in obj_paths {
        args.push(obj.to_string_lossy().to_string());
    }

    let output = Command::new("llvm-ar")
        .args(&args)
        .output()
        .map_err(|e| CompilerError::link(format!("Failed to run llvm-ar: {}", e)))?;

    if !output.status.success() {
        return Err(CompilerError::link(format!(
            "llvm-ar failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    println!("Created library: {}", lib_path.display());
    Ok(())
}

/// Link multiple object files into executable (Windows)
#[cfg(target_os = "windows")]
pub fn link_objects(object_files: &[PathBuf], sigil_libs: &[PathBuf], native_libs: &[PathBuf], output_path: &Path) -> Result<(), CompilerError> {
    let exe_path = if output_path.extension().is_some() {
        output_path.to_path_buf()
    } else {
        output_path.with_extension("exe")
    };
    // Create parent directory if needed (ignore error - linker will fail with clear message)
    if let Some(parent) = exe_path.parent() {
        let _: Result<(), _> = fs::create_dir_all(parent);
    }
    println!("Linking {}...", exe_path.display());

    // Locate the MSVC linker and library search paths WITHOUT hardcoding any
    // machine- or user-specific path. See find_windows_linker() below.
    let (linker, lib_paths) = find_windows_linker();

    let mut link_args = vec![
        "/NOLOGO".to_string(),
        "/SUBSYSTEM:CONSOLE".to_string(),
    ];
    for p in &lib_paths {
        link_args.push(format!("/LIBPATH:{}", p));
    }

    // Add all object files
    for obj in object_files {
        link_args.push(obj.to_string_lossy().to_string());
    }

    // Add Sigil libraries (parsed for symbols, also linked)
    for lib in sigil_libs {
        link_args.push(lib.to_string_lossy().to_string());
    }

    // Add native libraries (passed directly to linker)
    for lib in native_libs {
        link_args.push(lib.to_string_lossy().to_string());
    }

    link_args.push("msvcrt.lib".to_string());
    link_args.push("ucrt.lib".to_string());
    link_args.push("legacy_stdio_definitions.lib".to_string());
    link_args.push("kernel32.lib".to_string());
    link_args.push("ws2_32.lib".to_string());

    link_args.push(format!("/OUT:{}", exe_path.display()));

    let link_result = Command::new(&linker)
        .args(&link_args)
        .output();

    match link_result {
        Ok(output) => {
            if output.status.success() {
                println!("Successfully created {}", exe_path.display());
                Ok(())
            } else {
                Err(CompilerError::link(format!(
                    "linker '{}' failed (exit code {:?}):\n{}{}",
                    linker,
                    output.status.code(),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr),
                )))
            }
        }
        Err(e) => Err(CompilerError::link(format!(
            "failed to run linker '{}': {}. Run from a Visual Studio Developer \
             Command Prompt (or after vcvars64.bat), or install Visual Studio \
             Build Tools / LLVM (lld-link).",
            linker, e
        ))),
    }
}

/// Locate the Windows linker and any library search paths it needs, without
/// hardcoding machine- or user-specific paths. Strategy, in order:
///   1. If `link.exe` is on PATH and `LIB` is set, a Visual Studio developer
///      environment is active — use `link.exe` and let it read `LIB` itself.
///   2. Otherwise discover the install via `vswhere.exe` (at its stable,
///      documented location) and derive the MSVC tools dir + the newest
///      Windows SDK, passing those as explicit `/LIBPATH` entries.
///   3. Otherwise fall back to LLVM's `lld-link.exe` from PATH.
#[cfg(target_os = "windows")]
fn find_windows_linker() -> (String, Vec<String>) {
    // 1. Already inside a VS developer environment?
    if on_path("link.exe") && std::env::var_os("LIB").is_some() {
        return ("link.exe".to_string(), Vec::new());
    }
    // 2. Discover via vswhere.
    if let Some(found) = discover_via_vswhere() {
        return found;
    }
    // 3. lld-link fallback (relies on LIB env for library paths).
    ("lld-link.exe".to_string(), Vec::new())
}

/// Return true if `exe` is found in any directory on `PATH`.
#[cfg(target_os = "windows")]
fn on_path(exe: &str) -> bool {
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            if dir.join(exe).exists() {
                return true;
            }
        }
    }
    false
}

/// Use vswhere.exe to find the latest Visual Studio install and derive the
/// MSVC `link.exe` plus the MSVC and Windows SDK library directories.
#[cfg(target_os = "windows")]
fn discover_via_vswhere() -> Option<(String, Vec<String>)> {
    let pf86 = std::env::var("ProgramFiles(x86)")
        .or_else(|_| std::env::var("ProgramFiles"))
        .ok()?;
    let vswhere = format!(r"{}\Microsoft Visual Studio\Installer\vswhere.exe", pf86);
    if !Path::new(&vswhere).exists() {
        return None;
    }

    let out = Command::new(&vswhere)
        .args([
            "-latest",
            "-products", "*",
            "-property", "installationPath",
        ])
        .output()
        .ok()?;
    let install = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if install.is_empty() {
        return None;
    }

    // MSVC tools version (e.g. "14.40.33807").
    let ver_file = format!(
        r"{}\VC\Auxiliary\Build\Microsoft.VCToolsVersion.default.txt",
        install
    );
    let ver = fs::read_to_string(&ver_file).ok()?.trim().to_string();
    let tools = format!(r"{}\VC\Tools\MSVC\{}", install, ver);
    let link_exe = format!(r"{}\bin\Hostx64\x64\link.exe", tools);
    if !Path::new(&link_exe).exists() {
        return None;
    }

    let mut lib_paths = vec![format!(r"{}\lib\x64", tools)];
    if let Some((ucrt, um)) = latest_windows_sdk_libs(&pf86) {
        lib_paths.push(ucrt);
        lib_paths.push(um);
    }
    Some((link_exe, lib_paths))
}

/// Find the newest installed Windows 10/11 SDK and return its ucrt and um
/// x64 library directories.
#[cfg(target_os = "windows")]
fn latest_windows_sdk_libs(pf86: &str) -> Option<(String, String)> {
    let lib_root = format!(r"{}\Windows Kits\10\Lib", pf86);
    let mut versions: Vec<String> = fs::read_dir(&lib_root)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("10.") && Path::new(&lib_root).join(n).join("ucrt").exists())
        .collect();
    versions.sort();
    let v = versions.last()?;
    Some((
        format!(r"{}\{}\ucrt\x64", lib_root, v),
        format!(r"{}\{}\um\x64", lib_root, v),
    ))
}

/// Link multiple object files into executable (Linux/macOS)
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn link_objects(object_files: &[PathBuf], sigil_libs: &[PathBuf], native_libs: &[PathBuf], output_path: &Path) -> Result<(), CompilerError> {
    #[cfg(target_os = "linux")]
    const LINKER: &str = "gcc";
    #[cfg(target_os = "linux")]
    const LINKER_HINT: &str = "Make sure gcc is installed and in PATH";

    #[cfg(target_os = "macos")]
    const LINKER: &str = "clang";
    #[cfg(target_os = "macos")]
    const LINKER_HINT: &str = "Make sure Xcode Command Line Tools are installed";

    let exe_path = output_path.to_path_buf();
    // Create parent directory if needed (ignore error - linker will fail with clear message)
    if let Some(parent) = exe_path.parent() {
        let _: Result<(), _> = fs::create_dir_all(parent);
    }
    println!("Linking {}...", exe_path.display());

    let mut link_args = vec![
        "-o".to_string(),
        exe_path.to_string_lossy().to_string(),
    ];

    // Add all object files
    for obj in object_files {
        link_args.push(obj.to_string_lossy().to_string());
    }

    // Add Sigil libraries
    for lib in sigil_libs {
        link_args.push(lib.to_string_lossy().to_string());
    }

    // Add native libraries
    for lib in native_libs {
        link_args.push(lib.to_string_lossy().to_string());
    }

    link_args.push("-lpthread".to_string());

    match Command::new(LINKER).args(&link_args).output() {
        Ok(output) => {
            if output.status.success() {
                println!("Successfully created {}", exe_path.display());
                Ok(())
            } else {
                Err(CompilerError::link(format!(
                    "linker '{}' failed (exit code {:?}):\n{}{}",
                    LINKER,
                    output.status.code(),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr),
                )))
            }
        }
        Err(e) => Err(CompilerError::link(format!(
            "failed to run {}: {}. {}", LINKER, e, LINKER_HINT
        ))),
    }
}

/// Fallback for unsupported platforms
#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
pub fn link_objects(_object_files: &[PathBuf], _sigil_libs: &[PathBuf], _native_libs: &[PathBuf], _output_path: &Path) -> Result<(), CompilerError> {
    Err(CompilerError::link(
        "linking not supported on this platform; link the object files manually".to_string()
    ))
}
