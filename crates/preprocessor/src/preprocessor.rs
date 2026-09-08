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

    // For #import: only treat as a nupa import if the file is a nupa header
    // (.nh) or nupa implementation (.np).
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

/// A conditional-block frame on the `cond_stack`.
///
/// `active` is whether this specific branch emits, `any_met` whether an
/// earlier branch in the same chain was taken. For a *singleton self-guard*
/// (`#ifndef NAME` … `#define NAME` … `#endif` with nothing else inside) we
/// additionally remember the guard name and the define line so the guard can
/// be preserved verbatim in the C output. This lets the C compiler respect a
/// pre-existing definition of `NAME` (e.g. `nupa/runtime.h`'s `YES`/`NO`)
/// instead of redefining the macro (`-Wmacro-redefined`).
struct CondFrame {
    active: bool,
    any_met: bool,
    /// Some(NAME) while this frame is still a singleton-self-guard candidate.
    /// Cleared as soon as anything else (other content, `#else`, another
    /// define, …) appears inside the block.
    guard_name: Option<String>,
    /// The `#define NAME` line stashed while we wait for the matching `#endif`.
    pending_define: Option<String>,
    /// Set once the block contains anything other than the guarded define —
    /// disables preservation (behaves like a condition that is flattened away).
    dirty: bool,
}

/// A guard candidate that turned out not to be a pure singleton self-guard
/// is flushed back to the plain flattened behaviour: the definition is emitted
/// unconditionally (as before) and the guard directives stay dropped.
fn poison_top_guard(cond_stack: &mut Vec<CondFrame>, c_out: &mut Vec<String>) {
    if let Some(frame) = cond_stack.last_mut() {
        if frame.guard_name.is_some() || frame.pending_define.is_some() {
            frame.dirty = true;
            frame.guard_name = None;
            if let Some(def) = frame.pending_define.take() {
                c_out.push(def.trim().to_string());
            }
        }
    }
}

/// Recursively resolve #import and collect #include from a single file's content.
fn resolve_source(
    content: &str,
    file_path: &str,
    search_dirs: &[String],
    resolved: &mut HashSet<String>,
    nupa_out: &mut String,
    c_out: &mut Vec<String>,
    defined: &mut HashSet<String>,
    cond_stack: &mut Vec<CondFrame>,
) -> Result<(), String> {
    let dir = Path::new(file_path).parent()
        .and_then(|p| p.to_str())
        .unwrap_or(".")
        .to_string();

    for line in content.lines() {
        let trimmed = line.trim();
        // `active` = all blocks (including current) are emitting code.
        let active = cond_stack.iter().all(|f| f.active);
        // `parent_active` = all blocks except the innermost are emitting.
        // Used to decide #elif/#else (which replace the current branch), so the
        // current branch's own inactive state must not suppress re-evaluation.
        let parent_active = cond_stack[..cond_stack.len().saturating_sub(1)]
            .iter().all(|f| f.active);

        // Conditional directives: #ifdef / #ifndef / #if / #elif / #else / #endif
        if trimmed.starts_with('#') {
            let after = trimmed[1..].trim_start();
            if let Some(rest) = after.strip_prefix("ifdef") {
                let name = rest.trim().split(|c: char| c.is_whitespace() || c == '(' || c == ')')
                    .next().unwrap_or("").to_string();
                let truthy = parent_active && (defined.contains(&name) || predefined_macro(&name));
                cond_stack.push(CondFrame { active: truthy, any_met: truthy, guard_name: None, pending_define: None, dirty: false });
                continue;
            } else if let Some(rest) = after.strip_prefix("ifndef") {
                let name = rest.trim().split(|c: char| c.is_whitespace() || c == '(' || c == ')')
                    .next().unwrap_or("").to_string();
                let truthy = parent_active && !(defined.contains(&name) || predefined_macro(&name));
                cond_stack.push(CondFrame {
                    active: truthy, any_met: truthy,
                    // Only an active singleton `#ifndef` can be preserved.
                    guard_name: if truthy { Some(name) } else { None },
                    pending_define: None, dirty: false,
                });
                continue;
            } else if after.starts_with("if ") || after.starts_with("if\t") {
                let expr = after[2..].trim();
                let truthy = parent_active && eval_if_expr(expr, defined);
                cond_stack.push(CondFrame { active: truthy, any_met: truthy, guard_name: None, pending_define: None, dirty: false });
                continue;
            } else if after.starts_with("elif") {
                // An `#elif` inside a self-guard candidate forces flattening.
                poison_top_guard(cond_stack, c_out);
                if let Some(frame) = cond_stack.last_mut() {
                    frame.dirty = true;
                    frame.guard_name = None;
                    if frame.any_met {
                        frame.active = false;
                    } else {
                        let rest = after["elif".len()..].trim();
                        let expr = rest.strip_prefix("if ").or_else(|| rest.strip_prefix("if\t")).unwrap_or(rest);
                        let result = parent_active && eval_if_expr(expr.trim(), defined);
                        frame.active = result;
                        frame.any_met = result;
                    }
                }
                continue;
            } else if after.starts_with("else") {
                // An `#else` inside a self-guard candidate forces flattening.
                poison_top_guard(cond_stack, c_out);
                if let Some(frame) = cond_stack.last_mut() {
                    frame.dirty = true;
                    frame.guard_name = None;
                    frame.active = !frame.any_met;
                }
                continue;
            } else if after.starts_with("endif") {
                if let Some(frame) = cond_stack.pop() {
                    if let Some(name) = frame.guard_name {
                        if !frame.dirty {
                            // Pure singleton self-guard: preserve it so the C
                            // compiler honours a definition seen earlier
                            // (e.g. nupa/runtime.h's `YES`/`NO`).
                            if let Some(def) = frame.pending_define {
                                c_out.push(format!("#ifndef {}", name));
                                c_out.push(def.trim().to_string());
                                c_out.push("#endif".to_string());
                            }
                        } else if let Some(def) = frame.pending_define {
                            c_out.push(def.trim().to_string());
                        }
                    } else if let Some(def) = frame.pending_define {
                        c_out.push(def.trim().to_string());
                    }
                }
                continue;
            }
        }

        // Only process non-directive and active sections past this point.
        if !active {
            // Register #define even in inactive regions so macros resolve later.
            if let Some(rest) = trimmed.strip_prefix("#define") {
                let name = rest.trim().split_whitespace().next().unwrap_or("");
                if !name.is_empty() { defined.insert(name.to_string()); }
            }
            continue;
        }

        if let Some((is_nupa_import, name)) = is_directive(line) {
            // Anything inside a guard candidate besides the define it guards
            // forces the flat (non-preserved) emission.
            if is_nupa_import {
                poison_top_guard(cond_stack, c_out);
                // #import of .nh/.np (or .np) → recursively resolve
                let mut search = search_dirs.to_vec();
                // Add source directory first
                if !search.contains(&dir) {
                    search.insert(0, dir.clone());
                }
                resolve_imports(&name, &search, resolved, nupa_out, c_out, defined, cond_stack)?;
            } else {
                poison_top_guard(cond_stack, c_out);
                // #include → collect for C output (verbatim)
                let orig = line.trim().to_string();
                if !c_out.contains(&orig) {
                    c_out.push(orig);
                }
            }
        } else if line.trim_start().starts_with('#') {
            // Preprocessor directives: #define, #pragma, etc.
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
                poison_top_guard(cond_stack, c_out);
                let orig = line.to_string();
                c_out.push(orig.trim().to_string());
            } else if let Some(rest) = trimmed.strip_prefix("#define") {
                let name = rest.trim().split_whitespace().next().unwrap_or("").to_string();
                if !name.is_empty() { defined.insert(name.clone()); }
                // Singleton self-guard candidate: `#ifndef NAME` guarding
                // exactly `#define NAME …` — stash the define so the guard can
                // be preserved in the C output (avoids macro redefinition when
                // the macro was already defined by an included header).
                let is_guarded_define = cond_stack.last().map_or(false, |f| {
                    f.guard_name.as_deref() == Some(name.as_str())
                        && f.pending_define.is_none()
                        && !f.dirty
                });
                if is_guarded_define {
                    if let Some(frame) = cond_stack.last_mut() {
                        frame.pending_define = Some(line.to_string());
                    }
                } else {
                    poison_top_guard(cond_stack, c_out);
                    let orig = line.to_string();
                    c_out.push(orig.trim().to_string());
                }
            } else {
                // #pragma, #warning, etc.
                poison_top_guard(cond_stack, c_out);
                let orig = line.to_string();
                c_out.push(orig.trim().to_string());
            }
        } else {
            // Regular nupa source line. Blank lines and `//` comments are
            // inert — they do not break a pure singleton self-guard.
            let non_inert = !(trimmed.is_empty() || trimmed.starts_with("//"));
            if non_inert {
                poison_top_guard(cond_stack, c_out);
            }
            nupa_out.push_str(line);
            nupa_out.push('\n');
        }
    }
    Ok(())
}

/// Predefined macros (target platform/compiler). nupac always emits C that is
/// compiled by clang (or gcc via zig), so `__clang__`/`__GNUC__` are defined.
fn predefined_macro(name: &str) -> bool {
    matches!(name,
        "__APPLE__" | "__MACH__" | "__LP64__" | "__x86_64__" | "__aarch64__" | "__amd64__")
}

/// Evaluate a simplified `#if` expression: supports `defined(X)`, `!defined(X)`,
/// `X`, `!X`, integer comparisons `==`/`!=`/`<`/`>` and `&&`/`||`/`!`.
fn eval_if_expr(expr: &str, defined: &HashSet<String>) -> bool {
    let e = expr.trim();
    if e.is_empty() { return false; }
    // Strip outer parens
    let e = strip_outer_parens(e);
    // defined(X)
    if let Some(inner) = e.strip_prefix("defined(") {
        if let Some(name) = inner.strip_suffix(')') {
            return defined.contains(name.trim()) || predefined_macro(name.trim());
        }
    }
    if let Some(inner) = e.strip_prefix("!defined(") {
        if let Some(name) = inner.strip_suffix(')') {
            return !(defined.contains(name.trim()) || predefined_macro(name.trim()));
        }
    }
    // Logical NOT
    if let Some(rest) = e.strip_prefix('!') {
        return !eval_if_expr(rest, defined);
    }
    // Logical OR / AND (left-to-right, no precedence — good enough for simple macros)
    if let Some(idx) = rfind_token(e, "||") {
        return eval_if_expr(&e[..idx], defined) || eval_if_expr(&e[idx + 2..], defined);
    }
    if let Some(idx) = rfind_token(e, "&&") {
        return eval_if_expr(&e[..idx], defined) && eval_if_expr(&e[idx + 2..], defined);
    }
    // Integer comparison
    for (op, is_cmp) in [("==", true), ("!=", true), ("<", true), (">", true), ("<=", true), (">=", true)] {
        if let Some(idx) = find_op(e, op) {
            let l = eval_if_expr(&e[..idx], defined);
            let r = eval_if_expr(&e[idx + op.len()..], defined);
            let _ = is_cmp;
            return l == r;
        }
    }
    // Bare identifier or integer
    if e.parse::<i64>().is_ok() {
        return e.parse::<i64>().unwrap_or(0) != 0;
    }
    let name = e.split(|c: char| c.is_whitespace()).next().unwrap_or("").to_string();
    defined.contains(&name) || predefined_macro(&name)
}

fn strip_outer_parens(s: &str) -> &str {
    let mut t = s.trim();
    while t.starts_with('(') && t.ends_with(')') {
        let inner = &t[1..t.len() - 1];
        if inner.find('(').map_or(true, |_| true) {
            // Only strip if the parens are balanced around the whole string
            let mut depth = 0;
            for (i, ch) in t.char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' => { depth -= 1; if depth == 0 && i != t.len() - 1 { return s; } }
                    _ => {}
                }
            }
            t = inner;
        } else { break; }
    }
    t
}

fn rfind_token(s: &str, tok: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let tl = tok.len();
    let mut i = s.len();
    while i >= tl {
        i -= 1;
        if &s[i..i + tl] == tok {
            // ensure it's not part of a larger token (e.g. != contains =)
            return Some(i);
        }
        let _ = bytes;
    }
    None
}

fn find_op(s: &str, op: &str) -> Option<usize> {
    let idx = s.find(op)?;
    Some(idx)
}

/// Open a file and resolve its imports.
fn resolve_imports(
    name: &str,
    search_dirs: &[String],
    resolved: &mut HashSet<String>,
    nupa_out: &mut String,
    c_out: &mut Vec<String>,
    defined: &mut HashSet<String>,
    cond_stack: &mut Vec<CondFrame>,
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

    resolve_source(&content, &full_path, search_dirs, resolved, nupa_out, c_out, defined, cond_stack)
}

impl Preprocessor {
    /// Process a .np source file: resolve imports, collect headers.
    pub fn process_file(input_path: &str, search_dirs: &[String], extra_macros: &[&str]) -> Result<Preprocessor, String> {
        let content = fs::read_to_string(input_path)
            .map_err(|e| format!("cannot read {}: {}", input_path, e))?;

        Self::process(&content, input_path, search_dirs, extra_macros)
    }

    /// Process source text with import resolution.
    /// `extra_macros` are compiler-specific predefined macros (e.g. `__clang__`).
    pub fn process(content: &str, file_path: &str, search_dirs: &[String], extra_macros: &[&str]) -> Result<Preprocessor, String> {
        let mut resolved = HashSet::new();
        resolved.insert(file_path.to_string());

        let mut nupa_out = String::new();
        let mut c_out = Vec::new();
        let mut defined = HashSet::new();
        for m in extra_macros { defined.insert(m.to_string()); }
        let mut cond_stack: Vec<CondFrame> = Vec::new();

        resolve_source(content, file_path, search_dirs, &mut resolved, &mut nupa_out, &mut c_out, &mut defined, &mut cond_stack)?;

        Ok(Preprocessor {
            resolved_nupa: nupa_out,
            c_headers: c_out,
        })
    }
}