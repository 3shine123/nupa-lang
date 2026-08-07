use std::collections::HashSet;
use std::fs;
use std::path::Path;

pub struct Preprocessor {
    pub resolved_nupa: String,
    pub c_headers: Vec<String>,
}

impl Preprocessor {
    pub fn new() -> Self {
        Preprocessor {
            resolved_nupa: String::new(),
            c_headers: Vec::new(),
        }
    }
}

/// Check if a line is an #include or #import directive and extract the header name.
fn is_directive(line: &str) -> Option<(bool, String)> {
    let trimmed = line.trim();
    if !trimmed.starts_with('#') {
        return None;
    }
    let after_hash = trimmed[1..].trim_start();
    let is_import = if after_hash.starts_with("import") { true }
    else if after_hash.starts_with("include") { false }
    else { return None };

    let body = if is_import {
        &after_hash["import".len()..]
    } else {
        &after_hash["include".len()..]
    };
    let body = body.trim();

    let (start_char, end_char) = if body.starts_with('<') {
        ('<', '>')
    } else if body.starts_with('"') {
        ('"', '"')
    } else {
        return None;
    };

    let body = &body[1..]; // skip opening < or "
    let end = body.find(end_char)?;
    let name = body[..end].to_string();

    // For #import: only treat as nupa import if the file is .nh or .np
    let is_nupa_import = if is_import {
        let ext = Path::new(&name).extension().and_then(|e| e.to_str()).unwrap_or("");
        ext == "nh" || ext == "np"
    } else {
        false
    };

    Some((is_nupa_import, name))
}

/// Try to open a file, searching through multiple directories.
fn try_open(name: &str, search_dirs: &[String]) -> Option<String> {
    for dir in search_dirs {
        let path = format!("{}/{}", dir, name);
        if let Ok(content) = fs::read_to_string(&path) {
            return Some(content);
        }
    }
    // Also try the raw name
    fs::read_to_string(name).ok()
}

/// Recursively resolve #import and collect #include from a single file's content.
fn resolve_source(
    content: &str,
    file_path: &str,
    search_dirs: &[String],
    resolved: &mut HashSet<String>,
    nupa_out: &mut String,
    c_out: &mut Vec<String>,
) -> Result<(), String> {
    let dir = Path::new(file_path).parent()
        .and_then(|p| p.to_str())
        .unwrap_or(".")
        .to_string();

    for line in content.lines() {
        if let Some((is_nupa_import, name)) = is_directive(line) {
            if is_nupa_import {
                // #import of .nh/.np → recursively resolve
                let mut search = search_dirs.to_vec();
                // Add source directory first
                if !search.contains(&dir) {
                    search.insert(0, dir.clone());
                }
                resolve_imports(&name, &search, resolved, nupa_out, c_out)?;
            } else {
                // #include → collect for C output (verbatim)
                let orig = line.trim().to_string();
                if !c_out.contains(&orig) {
                    c_out.push(orig);
                }
            }
        } else if line.trim_start().starts_with('#') {
            // Preprocessor directives: #define, #ifdef, #ifndef, #endif, #pragma, etc.
            // Check if #define contains nupa message send syntax [receiver msg]
            // If so, keep it in Nupa source so the parser and codegen can process it.
            let is_define_with_nupa = if line.trim_start().starts_with("#define") {
                let line_body = line.trim_start();
                let line_body = &line_body["#define".len()..].trim();
                let value_start = line_body.find(char::is_whitespace)
                    .map(|i| line_body[i..].trim_start())
                    .unwrap_or("");
                value_start.contains('[') && value_start.contains(']')
            } else {
                false
            };
            if is_define_with_nupa {
                // #define with message send: NOT supported. The C compiler
                // doesn't understand [receiver msg] syntax, and the Nupa
                // compiler can't expand macros. Users should use inline code.
                let orig = line.to_string();
                c_out.push(orig.trim().to_string());
            } else {
                let orig = line.to_string();
                c_out.push(orig.trim().to_string());
            }
        } else {
            // Regular nupa source line
            nupa_out.push_str(line);
            nupa_out.push('\n');
        }
    }
    Ok(())
}

/// Open a file and resolve its imports.
fn resolve_imports(
    name: &str,
    search_dirs: &[String],
    resolved: &mut HashSet<String>,
    nupa_out: &mut String,
    c_out: &mut Vec<String>,
) -> Result<(), String> {
    // Try to find the file
    let content = try_open(name, search_dirs)
        .ok_or_else(|| format!("cannot open import: {}", name))?;

    // Resolve the full path for dedup
    let full_path = search_dirs.iter()
        .map(|d| format!("{}/{}", d, name))
        .find(|p| Path::new(p).exists())
        .unwrap_or_else(|| name.to_string());

    // Dedup: skip if already imported
    if resolved.contains(&full_path) {
        return Ok(());
    }
    resolved.insert(full_path.clone());

    resolve_source(&content, &full_path, search_dirs, resolved, nupa_out, c_out)
}

impl Preprocessor {
    /// Process a .np source file: resolve imports, collect headers.
    pub fn process_file(input_path: &str, search_dirs: &[String]) -> Result<Preprocessor, String> {
        let content = fs::read_to_string(input_path)
            .map_err(|e| format!("cannot read {}: {}", input_path, e))?;

        Self::process(&content, input_path, search_dirs)
    }

    /// Process source text with import resolution.
    pub fn process(content: &str, file_path: &str, search_dirs: &[String]) -> Result<Preprocessor, String> {
        let mut resolved = HashSet::new();
        resolved.insert(file_path.to_string());

        let mut nupa_out = String::new();
        let mut c_out = Vec::new();

        resolve_source(content, file_path, search_dirs, &mut resolved, &mut nupa_out, &mut c_out)?;

        Ok(Preprocessor {
            resolved_nupa: nupa_out,
            c_headers: c_out,
        })
    }
}