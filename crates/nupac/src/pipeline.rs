use std::path::Path;
use std::fs;
use nupa_parser::parser::Parser;
use nupa_binder::Binder;
use nupa_elaborator::Elaborator;
use nupa_codegen::{ast_to_cg_unit, emit_unit_with_headers, emit_bridge_header};
use nupa_preprocessor::Preprocessor;
use nupa_symbol::SymbolTable;
use nupa_ast::ast::*;
use nupa_cfg::cfg_build;
use nupa_arc::{arc_local_analyze, arc_global_analyze, arc_analyze_loops, arc_insert_actions, arc_optimize_pairs};
use nupa_checker::Checker;
use nupa_trace::{trace_refcounts, TraceOptions};
use attrs::{Backend, disposition, AttrDisposition};

pub struct Pipeline {
    pub has_error: bool,
    pub error_msg: String,
    pub search_dirs: Vec<String>,
    pub no_arc: bool,
    pub no_checker: bool,
    pub no_libc: bool,
    pub verbose: bool,
    pub trace_refcount: bool,
    pub trace_max_iters: usize,
    pub trace_color: bool,
    pub backend: Backend,
    pub werror: bool,
    pub bridge_header: Option<String>,
    pub no_comments: bool,
}

impl Pipeline {
    pub fn new() -> Self {
        Pipeline {
            has_error: false,
            error_msg: String::new(),
            search_dirs: vec!["include".into(), ".".into(), "include/Foundation".into()],
            no_arc: false,
            no_checker: false,
            no_libc: false,
            verbose: false,
            trace_refcount: false,
            trace_max_iters: 2,
            trace_color: true,
            backend: Backend::Clang,
            werror: false,
            bridge_header: None,
            no_comments: false,
        }
    }

    pub fn transpile_file(&mut self, input_path: &str, output_path: &str) -> Result<(), String> {
        let source = fs::read_to_string(input_path)
            .map_err(|e| format!("cannot read {}: {}", input_path, e))?;

        let c_code = self.transpile(&source, input_path)?;

        if let Some(parent) = Path::new(output_path).parent() {
            fs::create_dir_all(parent).map_err(|e| format!("cannot create output dir: {}", e))?;
        }
        fs::write(output_path, &c_code)
            .map_err(|e| format!("cannot write {}: {}", output_path, e))?;

        Ok(())
    }

    pub fn transpile(&mut self, source: &str, filename: &str) -> Result<String, String> {
        // `__NUPA__` is always defined: nupa headers can guard objc-style
        // syntax behind `#ifdef __NUPA__` so a plain C compiler sees only the
        // C-compatible subset when the header is used directly (without nupac).
        let extra_macros: &[&str] = match self.backend {
            attrs::Backend::Clang => &["__clang__", "__GNUC__", "__NUPA__"],
            attrs::Backend::Gcc => &["__GNUC__", "__NUPA__"],
            attrs::Backend::Portable => &["__GNUC__", "__NUPA__"],
        };
        let pre = Preprocessor::process(source, filename, &self.search_dirs, extra_macros)?;

        // Step 1: Parse the resolved nupa source
        if self.verbose { eprintln!("[nupac] parsing..."); }
        let mut parser = Parser::new(&pre.resolved_nupa);
        let mut cst = parser.parse_translation_unit()
            .ok_or_else(|| format!("Parse failed:\n{}", prefix_lines("[parser]",
                &translate_lines(parser.last_error(), &pre.source_map))))?;
        cst.filename = filename.to_string();

        if parser.has_error() {
            return Err(format!("Parse failed:\n{}", prefix_lines("[parser]",
                &translate_lines(parser.last_error(), &pre.source_map))));
        }

        // Step 2: Bind names
        if self.verbose { eprintln!("[nupac] binding names..."); }
        let symtab = SymbolTable::new();
        let mut binder = Binder::new(symtab);
        if binder.bind(&mut cst) != 0 {
            return Err(format!("Binding failed:\n{}", prefix_lines("[binder]",
                &translate_lines(binder.last_error(), &pre.source_map))));
        }

        // Step 3: Elaborate CST → AST
        if self.verbose { eprintln!("[nupac] elaborating..."); }
        let symtab_for_checker = binder.symtab.clone();
        let mut elaborator = Elaborator::new(Some(binder.symtab));
        elaborator.verbose = self.verbose;
        if elaborator.run(&cst) != 0 {
            return Err(format!("Elaboration failed:\n{}", prefix_lines("[elaborator]", elaborator.last_error())));
        }
        let mut ast = elaborator.take_ast()
            .ok_or_else(|| "Elaboration produced no AST".to_string())?;

        // Step 4: ARC analysis (skipped when -fno-nupa-arc is set)
        if self.verbose { eprintln!("[nupac] ARC analysis..."); }
        if !self.no_arc {
            for decl in &mut ast.decls {
                match &mut decl.data {
                    AstDeclData::Method { body: Some(ref mut b), .. } => {
                        if let Some(ref msym) = decl.name {
                            let cfg = cfg_build(b);
                            let mut arc_result = arc_local_analyze(b, &cfg, msym);
                            arc_global_analyze(&cfg, &mut arc_result, msym);
                            arc_analyze_loops(&cfg, &mut arc_result, msym);
                            arc_insert_actions(b, &arc_result);
                            arc_optimize_pairs(b);
                            for w in &arc_result.leak_warnings {
                                eprintln!("\x1b[1;35m[arc] warning:\x1b[0m {}:{}:{}: {}", filename, decl.line, decl.col, w);
                            }
                        }
                    }
                    AstDeclData::Function { body: Some(ref mut b), .. } => {
                        let name = decl.name.as_deref().unwrap_or("function");
                        let cfg = cfg_build(b);
                        let mut arc_result = arc_local_analyze(b, &cfg, name);
                        arc_global_analyze(&cfg, &mut arc_result, name);
                        arc_analyze_loops(&cfg, &mut arc_result, name);
                        arc_insert_actions(b, &arc_result);
                        arc_optimize_pairs(b);
                        for w in &arc_result.leak_warnings {
                            eprintln!("\x1b[1;35m[arc] warning:\x1b[0m {}:{}:{}: {}", filename, decl.line, decl.col, w);
                        }
                    }
                    AstDeclData::Class { methods, .. } => {
                        for m in methods {
                            if let AstDeclData::Method { body: Some(ref mut b), .. } = &mut m.data {
                                let name = m.name.as_deref().unwrap_or("method");
                                let cfg = cfg_build(b);
                                let mut arc_result = arc_local_analyze(b, &cfg, name);
                                arc_global_analyze(&cfg, &mut arc_result, name);
                                arc_analyze_loops(&cfg, &mut arc_result, name);
                                arc_insert_actions(b, &arc_result);
                                arc_optimize_pairs(b);
                                for w in &arc_result.leak_warnings {
                                    eprintln!("\x1b[1;35m[arc] warning:\x1b[0m {}:{}:{}: {}", filename, m.line, m.col, w);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Step 4.5: Reference-count trace (skips codegen entirely)
        if self.trace_refcount {
            if self.verbose { eprintln!("[nupac] tracing refcounts..."); }
            let opts = TraceOptions {
                max_iters: self.trace_max_iters,
                color: self.trace_color,
            };
            return Ok(trace_refcounts(&ast, &opts));
        }

        // Step 5: Check types (skipped when -fno-checker is set)
        if self.verbose { eprintln!("[nupac] checking types..."); }
        if !self.no_checker {
            let mut checker = Checker::new(Some(symtab_for_checker));
            checker.no_arc = self.no_arc;
            checker.source_map = Some(pre.source_map.clone());
            if checker.check(&mut ast) != 0 {
                return Err(format!("Type checking failed:\n{}", prefix_lines("[checker]", checker.last_error())));
            }
            // Print warnings (non-fatal diagnostics, like C/ObjC `-W...`).
            // With `-Werror`, suppress the purple warning and promote to a red error.
            if self.werror && !checker.warnings().is_empty() {
                return Err(format!("Type checking failed (-Werror):\n{}",
                    prefix_lines("[checker]", &checker.warnings().join("\n"))));
            }
            for w in checker.warnings() {
                eprintln!("\x1b[1;35m[checker] warning:\x1b[0m {}", w);
            }
        }

        // Step 5.5: Validate __attribute__ against backend
        if self.verbose { eprintln!("[nupac] validating attributes..."); }
        self.validate_attrs(&ast);
        if self.has_error {
            return Err(self.error_msg.clone());
        }

        // Step 6: Generate C code
        if self.verbose { eprintln!("[nupac] generating C code..."); }
        let cg = ast_to_cg_unit(&ast, self.backend);
        let c_code = emit_unit_with_headers(&cg, &pre.c_headers, &self.search_dirs, self.no_libc, self.backend, !self.no_comments);

        // Step 6.5: Generate bridge header (if requested)
        if let Some(ref path) = self.bridge_header {
            if self.verbose { eprintln!("[nupac] writing bridge header: {}", path); }
            let bridge = emit_bridge_header(&cg);
            fs::write(path, &bridge)
                .map_err(|e| format!("cannot write bridge header {}: {}", path, e))?;
        }

    Ok(c_code)
    }

    fn validate_attrs(&mut self, ast: &AstUnit) {
        for decl in &ast.decls {
            self.validate_decl_attrs(decl);
        }
    }

    fn validate_decl_attrs(&mut self, decl: &AstDecl) {
        for attr in &decl.attributes {
            let name = attr.split('(').next().unwrap_or(attr).trim();
            match disposition(name, self.backend) {
                AttrDisposition::Error => {
                    self.has_error = true;
                    self.error_msg = format!(
                        "{}:{}: error: attribute '{}' not allowed in --backend={} (try --backend=clang or --backend=gcc)",
                        decl.line, decl.col, name, self.backend
                    );
                    eprintln!("{}", self.error_msg);
                }
                AttrDisposition::Warn => {
                    eprintln!("\x1b[1;35m{}:{}: warning:\x1b[0m unknown attribute '{}' (passing through to C compiler)",
                        decl.line, decl.col, name);
                }
                AttrDisposition::Pass => {}
            }
        }
        // Recurse into nested decls (class methods, ivars, struct fields, etc.)
        match &decl.data {
            AstDeclData::Class { methods, ivars, properties, impl_vars, .. } => {
                for m in methods { self.validate_decl_attrs(m); }
                for iv in ivars { self.validate_decl_attrs(iv); }
                for p in properties { self.validate_decl_attrs(p); }
                for v in impl_vars { self.validate_decl_attrs(v); }
            }
            AstDeclData::Aggregate { fields } => {
                for f in fields { self.validate_decl_attrs(f); }
            }
            AstDeclData::Namespace(decls) => {
                for d in decls { self.validate_decl_attrs(d); }
            }
            _ => {}
        }
    }
}


/// Prefix every non-empty line of a multi-error message with `[stage]` so a
/// single compile can be read as a categorized report (parser/binder/checker…).
fn prefix_lines(stage: &str, msg: &str) -> String {
    msg.lines()
        .map(|l| if l.trim().is_empty() {
            l.to_string()
        } else {
            format!("{} {}", stage, l)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Rewrites `LINE:COL: message` occurrences in a diagnostic string so the
/// line points at the original source file (via the preprocessor's line map)
/// instead of the flattened inlined buffer. Lines that can't be mapped are
/// left untouched.
fn translate_lines(msg: &str, sm: &nupa_cst::source_map::SourceMap) -> String {
    if sm.is_empty() { return msg.to_string(); }
    msg.lines().map(|l| {
        // Parse leading `LINE:COL: ` (or `LINE: `)
        let mut parts = l.splitn(3, ':');
        let line_no: Option<usize> = parts.next().and_then(|p| p.trim().parse().ok());
        let rest: String = match (parts.next(), parts.next()) {
            (Some(col), Some(text)) => {
                let col_trim = col.trim();
                if col_trim.chars().all(|c| c.is_ascii_digit()) && col_trim.starts_with(|c: char| c.is_ascii_digit()) {
                    format!("{}: {}", col_trim, text)
                } else {
                    format!("{}: {}", col, text)
                }
            }
            (Some(col), None) => col.to_string(),
            _ => return l.to_string(),
        };
        match line_no {
            Some(n) if n > 0 => {
                let (file, real) = sm.locate(n);
                if file.is_empty() {
                    l.to_string()
                } else {
                    format!("{}:{}: {}", file, real, rest.trim_start())
                }
            }
            _ => l.to_string(),
        }
    }).collect::<Vec<_>>().join("\n")
}


