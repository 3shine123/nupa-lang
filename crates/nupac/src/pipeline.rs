use std::path::Path;
use std::fs;
use nupa_parser::parser::Parser;
use nupa_binder::Binder;
use nupa_elaborator::Elaborator;
use nupa_codegen::{ast_to_cg_unit, emit_unit_with_headers, emit_bridge_header};
use nupa_preprocessor::Preprocessor;
use nupa_symbol::SymbolTable;
use nupa_cst::TranslationUnit;
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
            .ok_or_else(|| format!("Parse failed:\n{}", prefix_lines("[parser]", parser.last_error())))?;
        cst.filename = filename.to_string();

        if parser.has_error() {
            return Err(format!("Parse failed:\n{}", prefix_lines("[parser]", parser.last_error())));
        }

        // Step 2: Bind names
        if self.verbose { eprintln!("[nupac] binding names..."); }
        let symtab = SymbolTable::new();
        let mut binder = Binder::new(symtab);
        if binder.bind(&mut cst) != 0 {
            return Err(format!("Binding failed:\n{}", prefix_lines("[binder]", binder.last_error())));
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

fn dump_ast_decl(d: &nupa_ast::AstDecl, indent: usize) {
    let sp = "  ".repeat(indent);
    eprintln!("{}kind={:?} name={:?}", sp, d.kind, d.name);
    match &d.data {
        nupa_ast::AstDeclData::Variable { var_type, init, is_block_qual, is_weak, .. } => {
            eprintln!("{}  var_type={:?} init={:?}", sp, var_type, init);
            if let Some(e) = init {
                dump_ast_expr(e, indent + 2);
            }
        }
        nupa_ast::AstDeclData::Function { return_type, params, body, .. } => {
            eprintln!("{}  return={:?}", sp, return_type);
            if let Some(p) = params {
                let mut q = p.next.as_ref();
                let mut idx = 0;
                while let Some(param) = q {
                    eprintln!("{}  param[{}]: name={:?} type={:?}", sp, idx, param.name, param.par_type);
                    q = param.next.as_ref();
                    idx += 1;
                }
            }
            if let Some(b) = body {
                eprintln!("{}  body:", sp);
                dump_ast_stmt(b, indent + 2);
            }
        }
        nupa_ast::AstDeclData::Class { methods, .. } => {
            for m in methods {
                eprintln!("{}  method:", sp);
                dump_ast_decl(m, indent + 3);
            }
        }
        nupa_ast::AstDeclData::Method { is_class_method, return_type, params, body, .. } => {
            eprintln!("{}  is_class={} return={:?}", sp, is_class_method, return_type);
            if let Some(b) = body {
                eprintln!("{}  body:", sp);
                dump_ast_stmt(b, indent + 2);
            }
        }
        _ => {}
    }
}

fn dump_ast_stmt(s: &nupa_ast::AstStmt, indent: usize) {
    let sp = "  ".repeat(indent);
    match &s.data {
        nupa_ast::AstStmtData::Expr(e) => {
            eprintln!("{}expr:", sp);
            dump_ast_expr(e, indent + 1);
        }
        nupa_ast::AstStmtData::Return(v) => {
            eprintln!("{}return:", sp);
            if let Some(e) = v {
                dump_ast_expr(e, indent + 1);
            }
        }
        nupa_ast::AstStmtData::Decl(d) => {
            eprintln!("{}decl:", sp);
            dump_ast_decl(d, indent + 1);
        }
        nupa_ast::AstStmtData::Compound(stmts) => {
            eprintln!("{}compound:", sp);
            for st in stmts {
                dump_ast_stmt(st, indent + 1);
            }
        }
        _ => eprintln!("{}stmt kind={:?}", sp, s.kind),
    }
}

fn dump_ast_expr(e: &nupa_ast::AstExpr, indent: usize) {
    let sp = "  ".repeat(indent);
    match &e.data {
        nupa_ast::AstExprData::VarRef { name, .. } => eprintln!("{}VarRef({})", sp, name),
        nupa_ast::AstExprData::IvarRef { ivar, obj, .. } => {
            eprintln!("{}IvarRef({:?}) obj:", sp, ivar);
            dump_ast_expr(obj, indent + 1);
        }
        nupa_ast::AstExprData::PropRef { name, obj, .. } => {
            eprintln!("{}PropRef({}) obj:", sp, name);
            dump_ast_expr(obj, indent + 1);
        }
        nupa_ast::AstExprData::MsgSend { receiver, selector, is_class_method, args, .. } => {
            eprintln!("{}MsgSend(selector={} class={}) receiver:", sp, selector, is_class_method);
            dump_ast_expr(receiver, indent + 1);
            for a in args {
                dump_ast_expr(a, indent + 1);
            }
        }
        nupa_ast::AstExprData::FuncCall { name, args, .. } => {
            eprintln!("{}FuncCall({})", sp, name);
            for a in args {
                dump_ast_expr(a, indent + 1);
            }
        }
        nupa_ast::AstExprData::Assign { target, value } => {
            eprintln!("{}Assign target:", sp);
            dump_ast_expr(target, indent + 1);
            eprintln!("{}value:", sp);
            dump_ast_expr(value, indent + 1);
        }
        nupa_ast::AstExprData::Binary { op, left, right } => {
            eprintln!("{}Binary(op={}) left:", sp, op);
            dump_ast_expr(left, indent + 1);
            eprintln!("{}right:", sp);
            dump_ast_expr(right, indent + 1);
        }
        nupa_ast::AstExprData::Ternary { cond, then, else_ } => {
            eprintln!("{}Ternary cond:", sp);
            dump_ast_expr(cond, indent + 1);
            eprintln!("{}then:", sp);
            dump_ast_expr(then, indent + 1);
            eprintln!("{}else:", sp);
            dump_ast_expr(else_, indent + 1);
        }
        nupa_ast::AstExprData::Unary { op, operand, is_postfix } => {
            eprintln!("{}Unary(op={} postfix={})", sp, op, is_postfix);
            dump_ast_expr(operand, indent + 1);
        }
        nupa_ast::AstExprData::FuncCall { name, args, .. } => {
            eprintln!("{}Call({})", sp, name);
            for a in args {
                dump_ast_expr(a, indent + 1);
            }
        }
        nupa_ast::AstExprData::Int(v) => eprintln!("{}Int({})", sp, v),
        nupa_ast::AstExprData::String(s) => eprintln!("{}String({})", sp, s),
        nupa_ast::AstExprData::Cast { target_type, expr } => {
            eprintln!("{}Cast target={:?}", sp, target_type);
            dump_ast_expr(expr, indent + 1);
        }
        nupa_ast::AstExprData::Comma(exprs) => {
            eprintln!("{}Comma:", sp);
            for e in exprs {
                dump_ast_expr(e, indent + 1);
            }
        }
        _ => eprintln!("{}expr kind={:?}", sp, e.kind),
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


