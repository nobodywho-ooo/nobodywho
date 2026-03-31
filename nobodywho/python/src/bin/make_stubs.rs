/// This file builds stubs for the nobodywho-python crate.
/// It uses the pyo3-introspection crate to introspect the nobodywho-python crate and
/// generate stubs for the Python bindings. The script should be run after the crate is built.
///
/// Usage:
///   cargo run --bin make_stubs [--release]
use pyo3_introspection::{introspect_cdylib, module_stub_files};
use std::collections::HashMap;
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

/// Parse `///` doc comments from Rust source, returning a map from
/// `(Option<class_name>, method_or_struct_name)` to the docstring text.
fn extract_rust_docs(src: &str) -> HashMap<(Option<String>, String), String> {
    let mut docs: HashMap<(Option<String>, String), String> = HashMap::new();
    let mut pending_docs: Vec<String> = Vec::new();
    let mut current_class: Option<String> = None;
    let mut just_saw_pymethods = false;
    // When #[new] is seen, the next fn should be recorded as "__new__"
    let mut next_fn_name_override: Option<String> = None;

    for line in src.lines() {
        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix("/// ") {
            pending_docs.push(rest.to_string());
        } else if trimmed == "///" {
            pending_docs.push(String::new());
        } else if trimmed.starts_with("#[pymethods") {
            just_saw_pymethods = true;
            pending_docs.clear();
        } else if trimmed.starts_with("#[new]") {
            next_fn_name_override = Some("__new__".to_string());
        } else if trimmed.starts_with("#[") {
            // Other attribute macros (#[pyo3(...)], #[staticmethod], #[pyclass], etc.)
            // Don't clear pending docs - they accumulate through attribute lines
        } else if trimmed.starts_with("impl ") && just_saw_pymethods {
            if let Some(name) = extract_impl_name(trimmed) {
                current_class = Some(name);
            }
            just_saw_pymethods = false;
            pending_docs.clear();
        } else if trimmed.starts_with("pub struct ") {
            if !pending_docs.is_empty() {
                if let Some(name) = extract_struct_name(trimmed) {
                    docs.insert((None, name), format_doc_lines(&pending_docs));
                }
                pending_docs.clear();
            }
            next_fn_name_override = None;
        } else if trimmed.starts_with("pub fn ")
            || trimmed.starts_with("pub async fn ")
            || trimmed.starts_with("async fn ")
        {
            if !pending_docs.is_empty() {
                if let Some(raw_name) = extract_fn_name(trimmed) {
                    let py_name = next_fn_name_override.take().unwrap_or(raw_name);
                    docs.insert(
                        (current_class.clone(), py_name),
                        format_doc_lines(&pending_docs),
                    );
                }
            } else {
                next_fn_name_override = None;
            }
            pending_docs.clear();
        } else if trimmed.is_empty() || trimmed.starts_with("//") {
            // Blank lines and non-doc comments don't clear pending docs
        } else {
            // Any other code clears the pending doc buffer
            pending_docs.clear();
            next_fn_name_override = None;
        }
    }

    docs
}

fn extract_impl_name(line: &str) -> Option<String> {
    let rest = line.strip_prefix("impl ")?.trim();
    // Skip leading lifetime/generic params like <'py>
    let rest = if rest.starts_with('<') {
        let end = rest.find('>')?;
        rest[end + 1..].trim()
    } else {
        rest
    };
    // First token is the type name; strip any trailing generics like <'_>
    let name = rest.split_whitespace().next()?;
    Some(name.split('<').next()?.to_string())
}

fn extract_struct_name(line: &str) -> Option<String> {
    let rest = line.strip_prefix("pub struct ")?;
    let name = rest.split_whitespace().next()?;
    // Strip trailing generics or braces
    Some(
        name.split(|c| c == '<' || c == '{')
            .next()?
            .trim()
            .to_string(),
    )
}

fn extract_fn_name(line: &str) -> Option<String> {
    let rest = if let Some(s) = line.strip_prefix("pub async fn ") {
        s
    } else if let Some(s) = line.strip_prefix("pub fn ") {
        s
    } else if let Some(s) = line.strip_prefix("async fn ") {
        s
    } else {
        return None;
    };
    // Name is everything up to the first `(` or `<`
    let name = rest.split(|c| c == '(' || c == '<').next()?.trim();
    Some(name.to_string())
}

fn format_doc_lines(lines: &[String]) -> String {
    let mut lines = lines.to_vec();
    // Trim trailing empty lines
    while lines.last().map(|l: &String| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    lines.join("\n")
}

/// Render a docstring block at the given indentation level.
/// Single-line docs: `    """text"""`
/// Multi-line docs:
/// ```
///     """First line.
///
///     More details.
///     """
/// ```
fn render_docstring(doc: &str, indent: &str) -> String {
    let lines: Vec<&str> = doc.lines().collect();
    if lines.len() <= 1 {
        let text = lines.first().copied().unwrap_or("");
        format!("{}\"\"\"{}\"\"\"", indent, text)
    } else {
        let first = lines[0];
        let rest: Vec<String> = lines[1..]
            .iter()
            .map(|l| {
                if l.is_empty() {
                    String::new()
                } else {
                    format!("{}{}", indent, l)
                }
            })
            .collect();
        format!(
            "{}\"\"\"{}\n{}\n{}\"\"\"",
            indent,
            first,
            rest.join("\n"),
            indent
        )
    }
}

/// Inject docstrings into the generated `.pyi` content using the `docs` lookup map.
fn inject_docstrings(content: &str, docs: &HashMap<(Option<String>, String), String>) -> String {
    let mut result: Vec<String> = Vec::new();
    let mut current_class: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim();

        // Detect top-level class definitions: "class ClassName:" or "class ClassName(Base):"
        if !line.starts_with(' ') && trimmed.starts_with("class ") && trimmed.ends_with(':') {
            let class_name = trimmed
                .strip_prefix("class ")
                .and_then(|s| s.strip_suffix(':'))
                .map(|s| s.split('(').next().unwrap_or(s).trim())
                .unwrap_or("")
                .to_string();

            result.push(line.to_string());
            current_class = Some(class_name.clone());

            if let Some(doc) = docs.get(&(None, class_name)) {
                result.push(render_docstring(doc, "    "));
            }
        }
        // Detect method/function def lines that end with ": ..."
        else if trimmed.starts_with("def ") && line.ends_with(": ...") {
            let fn_name = trimmed
                .strip_prefix("def ")
                .and_then(|s| s.split('(').next())
                .unwrap_or("")
                .to_string();

            let key = (current_class.clone(), fn_name);

            if let Some(doc) = docs.get(&key) {
                let indent_len = line.len() - line.trim_start().len();
                let indent = " ".repeat(indent_len);
                let inner_indent = " ".repeat(indent_len + 4);
                // Replace trailing ": ..." with ":\n    """doc"""\n    ..."
                let base = &line[..line.len() - 5]; // strip ": ..."
                result.push(format!("{}:", base));
                result.push(render_docstring(doc, &inner_indent));
                result.push(format!("{}...", inner_indent));
            } else {
                result.push(line.to_string());
            }
        } else {
            result.push(line.to_string());
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

    // Parse doc comments from the Rust source to inject into stubs
    let rust_src_path = "src/lib.rs";
    let rust_src = fs::read_to_string(rust_src_path)
        .map_err(|e| format!("Failed to read {}: {}", rust_src_path, e))?;
    let rust_docs = extract_rust_docs(&rust_src);
    println!(
        "Extracted {} doc entries from {}",
        rust_docs.len(),
        rust_src_path
    );

    let module = introspect_cdylib(&library_path, "nobodywho")?;
    let stub_files = module_stub_files(&module);

    // Output directory (can be overridden with STUBS_DIR env var)
    let output_dir = env::var("STUBS_DIR").unwrap_or_else(|_| ".".to_string());

    println!("Generating stub files in: {}", output_dir);
    for (file_path, contents) in stub_files.clone().iter() {
        let contents = replace_exceptions(contents.clone());
        let contents = inject_typevars(contents);
        let contents = inject_docstrings(&contents, &rust_docs);
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
