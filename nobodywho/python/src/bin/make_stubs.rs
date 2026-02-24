/// This file builds stubs for the nobodywho-python crate.
/// It uses the pyo3-introspection crate to introspect the nobodywho-python crate and
/// generate stubs for the Python bindings. The script should be run after the crate is built.
///
/// Usage:
///   cargo run --bin make_stubs [--release]
use pyo3_introspection::{introspect_cdylib, module_stub_files};
use std::env;
use std::fs;
use std::path::PathBuf;

const EXCEPTIONS_TO_REPLACE: &[(&str, &str)] = &[
    (
        "def __next__(self, /) -> typing.Any: ...",
        "def __next__(self, /) -> str: ... # Replaced with str to avoid type errors",
    ),
    (
        "def __anext__(self, /) -> typing.Any: ...",
        "def __anext__(self, /) -> typing.Awaitable[str]: ... # Replaced with str to avoid type errors",
    ),
];

const TYPEVARS_TO_INJECT: &[(&str, &str)] = &[(
    "T",
    "typing.TypeVar('T', str, typing.Awaitable[str])  # Type variable for tool return types (sync str or async Awaitable[str])",
)];

const GENERIC_TYPE_REPLACEMENTS: &[(&str, &str)] = &[
    ("class Tool:", "class Tool(typing.Generic[T]):"),
    (
        "typing.Callable[[typing.Callable[..., T]], Tool]",
        "typing.Callable[[typing.Callable[..., T]], Tool[T]]",
    ),
];

fn replace_exceptions(mut contents: String) -> String {
    for (pattern, replacement) in EXCEPTIONS_TO_REPLACE {
        contents = contents.replace(pattern, replacement);
    }
    contents
}

fn inject_typevars(mut contents: String) -> String {
    // First apply generic type replacements
    for (pattern, replacement) in GENERIC_TYPE_REPLACEMENTS {
        contents = contents.replace(pattern, replacement);
    }

    // Find the last import statement and inject TypeVar definitions after it
    let lines: Vec<&str> = contents.lines().collect();
    let mut result = Vec::new();
    let mut last_import_idx = None;

    // Find the last line that starts with "import " or "from "
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") || trimmed.starts_with("from ") {
            last_import_idx = Some(idx);
        }
    }

    // Inject TypeVars after the last import
    for (idx, line) in lines.iter().enumerate() {
        result.push(line.to_string());
        if Some(idx) == last_import_idx {
            result.push(String::new());
            for (var_name, var_definition) in TYPEVARS_TO_INJECT {
                result.push(format!("{} = {}", var_name, var_definition));
            }
        }
    }

    result.join("\n")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Determine build profile (debug or release)
    let profile = if env::var("PROFILE").unwrap_or_default() == "release" {
        "release"
    } else {
        "debug"
    };

    // Determine library extension based on platform
    let lib_ext = if cfg!(target_os = "macos") {
        "dylib"
    } else if cfg!(target_os = "windows") {
        "dll"
    } else {
        "so"
    };

    // Construct library path
    let library_path = format!("../target/{}/libnobodywho_python.{}", profile, lib_ext);

    // Check if library exists
    if !PathBuf::from(&library_path).exists() {
        eprintln!("Error: Library not found at {}", library_path);
        eprintln!("Please build the project first with: cargo build [--release]");
        std::process::exit(1);
    }

    let module = introspect_cdylib(&library_path, "nobodywho")?;
    let stub_files = module_stub_files(&module);

    // Output directory (can be overridden with STUBS_DIR env var)
    let output_dir = env::var("STUBS_DIR").unwrap_or_else(|_| ".".to_string());

    println!("Generating stub files in: {}", output_dir);
    for (file_path, contents) in stub_files.clone().iter() {
        let contents = replace_exceptions(contents.clone());
        let contents = inject_typevars(contents);
        let full_path = PathBuf::from(&output_dir).join(file_path);

        if contents.contains(" typing.Any") {
            println!("--- type stubs content: ---");
            println!("{}", contents);
            println!("--- end type stubs content ---");
            eprintln!(
                "❌ Error: typing.Any found in contents of {}. Please replace all typing.Any with the appropriate type.",
                full_path.display()
            );
            std::process::exit(1);
        } else {
            println!("✅ No anys found in {}!", full_path.display());
        }

        fs::write(&full_path, contents)?;
        println!("  Generated: {}", full_path.display());
    }

    // Move __init__.pyi to nobodywho.pyi
    let init_path = PathBuf::from(&output_dir).join("__init__.pyi");
    let target_path = PathBuf::from(&output_dir).join("nobodywho.pyi");

    if init_path.exists() {
        fs::rename(&init_path, &target_path)?;
        println!(
            "  Moved: {} -> {}",
            init_path.display(),
            target_path.display()
        );
    }

    println!("Done! Generated {} stub file(s)", stub_files.clone().len());
    Ok(())
}
