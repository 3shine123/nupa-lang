use nupa_ast::ast::*;
use nupa_cst::{TypePrim, CstParam};
use nupa_symbol::*;
use std::collections::HashMap;

/// Type checker for Nupa programs.
/// Validates types and reports type errors.
pub struct Checker {
    pub symtab: Option<SymbolTable>,
    pub current_class: Option<String>,
    pub current_method: Option<String>,
    pub has_error: bool,
    pub error_count: i32,
    pub error_msg: String,
    /// Non-fatal diagnostics (warnings). Collected like errors but do NOT set
    /// `has_error` — the pipeline decides whether to promote them via `-Werror`.
    pub warnings: Vec<String>,
    pub scope_vars: Vec<Vec<(String, AstType)>>,
    pub no_arc: bool,
    pub in_noarc: bool,
    /// Selector → param types (from the AST's method declarations), used to
    /// reject bare C-string literals passed to object-typed parameters.
    pub method_params: HashMap<String, Vec<Option<AstType>>>,
    /// Function name → param types (from AST function declarations).
    pub function_params: HashMap<String, Vec<Option<AstType>>>,
    /// Selector → is_class_method (from AST declarations), used to reject
    /// calling a class method on an instance or an instance method on a class.
    pub method_kinds: HashMap<String, bool>,
}

impl Checker {
    pub fn new(symtab: Option<SymbolTable>) -> Self {
        Checker {
            symtab,
            current_class: None,
            current_method: None,
            has_error: false,
            error_count: 0,
            error_msg: String::new(),
            warnings: Vec::new(),
            scope_vars: Vec::new(),
            no_arc: false,
            in_noarc: false,
            method_params: HashMap::new(),
            function_params: HashMap::new(),
            method_kinds: HashMap::new(),
        }
    }

    pub fn has_error(&self) -> bool {
        self.has_error
    }

    pub fn last_error(&self) -> &str {
        &self.error_msg
    }

    fn check_error(&mut self, line: usize, col: usize, msg: &str) {
        self.has_error = true;
        self.error_count += 1;
        let entry = format!("{}:{}: {}", line, col, msg);
        if self.error_msg.is_empty() {
            self.error_msg = entry;
        } else {
            self.error_msg = format!("{}\n{}", self.error_msg, entry);
        }
    }

    fn check_warning(&mut self, line: usize, col: usize, msg: &str) {
        let entry = format!("{}:{}: {}", line, col, msg);
        self.warnings.push(entry);
    }

    fn is_numeric_type(prim: TypePrim) -> bool {
        matches!(prim, TypePrim::Char | TypePrim::Short | TypePrim::Int | TypePrim::Long
            | TypePrim::LongLong | TypePrim::Float | TypePrim::Double | TypePrim::Bool
            | TypePrim::Signed | TypePrim::Unsigned)
    }

    /// Is this a Foundation/object pointer type (id, instancetype, or a named
    /// class pointer like `NPString *`)? Used to reject bare C-string literals
    /// (`"..."`) where an NPString/object is expected — mirroring ObjC, where
    /// `NSLog(NSString *, ...)` rejects `const char *` at the C type level.
    fn is_object_type(t: &AstType) -> bool {
        t.prim == TypePrim::Id
            || t.prim == TypePrim::Instancetype
            || (t.prim == TypePrim::Named && t.is_pointer)
    }

    /// True if `name` refers to a definitely-instance value: a local variable
    /// in scope, a method parameter, or an ivar of the current class.
    /// (Specifically NOT a class name and NOT a dynamic `[... class]` expression.)
    fn is_local_var(&self, name: &str) -> bool {
        for scope in self.scope_vars.iter().rev() {
            for (n, _) in scope.iter() {
                if n == name { return true; }
            }
        }
        if name == "self" || name == "_self" { return true; }
        if let (Some(ref cls_name), Some(ref st)) = (&self.current_class, &self.symtab) {
            if let Some(csym) = st.find_class(cls_name) {
                if let SymbolData::Class { ref ivars, .. } = csym.data {
                    if ivars.contains(&name.to_string()) { return true; }
                }
            }
        }
        false
    }

    /// Collect method and function signatures from the AST into the
    /// `method_params` / `function_params` maps.  Runs once before checking.
    fn collect_signatures(&mut self, decl: &AstDecl) {
        match &decl.data {
            AstDeclData::Class { methods, .. } => {
                for m in methods {
                    if let AstDeclData::Method { params, is_class_method, .. } = &m.data {
                        if let Some(ref sel) = m.name {
                            let mut v = Vec::new();
                            let mut cur = params.as_ref().map(|b| &**b);
                            while let Some(p) = cur {
                                v.push(p.par_type.as_ref().map(|pt| Self::cst_type_to_ast_type(pt)));
                                cur = p.next.as_ref().map(|n| &**n);
                            }
                            self.method_params.insert(sel.clone(), v);
                            self.method_kinds.insert(sel.clone(), *is_class_method);
                        }
                    }
                }
            }
            AstDeclData::Method { params, is_class_method, .. } => {
                if let Some(ref sel) = decl.name {
                    let mut v = Vec::new();
                    let mut cur = params.as_ref().map(|b| &**b);
                    while let Some(p) = cur {
                        v.push(p.par_type.as_ref().map(|pt| Self::cst_type_to_ast_type(pt)));
                        cur = p.next.as_ref().map(|n| &**n);
                    }
                    self.method_params.insert(sel.clone(), v);
                    self.method_kinds.insert(sel.clone(), *is_class_method);
                }
            }
            AstDeclData::Function { params, .. } => {
                if let Some(ref name) = decl.name {
                    let mut v = Vec::new();
                    let mut cur = params.as_ref().map(|b| &**b);
                    while let Some(p) = cur {
                        v.push(p.par_type.as_ref().map(|pt| Self::cst_type_to_ast_type(pt)));
                        cur = p.next.as_ref().map(|n| &**n);
                    }
                    self.function_params.insert(name.clone(), v);
                }
            }
            _ => {}
        }
    }

    fn cst_type_to_ast_type(ct: &nupa_cst::CstType) -> AstType {
        AstType::from_cst_type(ct)
    }

    /// Check an expression, set its expr_type, and return the type.
    pub fn check_expr(&mut self, e: &mut AstExpr) -> Option<AstType> {
        let result = self.check_expr_inner(e);
        e.expr_type = result.clone().map(Box::new);
        result
    }

    fn check_expr_inner(&mut self, e: &mut AstExpr) -> Option<AstType> {
        match &mut e.data {
            AstExprData::Int(_) => Some(AstType::new(TypePrim::Int)),
            AstExprData::Float(_) => Some(AstType::new(TypePrim::Double)),
            AstExprData::String(_) => {
                let mut t = AstType::new(TypePrim::Char);
                t.is_pointer = true;
                Some(t)
            }
            AstExprData::AtString(_) => {
                let mut t = AstType::new(TypePrim::Id);
                t.is_pointer = true;
                Some(t)
            }
            AstExprData::Char(_) => Some(AstType::new(TypePrim::Char)),
            AstExprData::Bool(_) => Some(AstType::new(TypePrim::Bool)),
            AstExprData::VarRef { name, .. } => {
                if name.starts_with("__") {
                    return Some(AstType::new(TypePrim::Int));
                }
                for scope in self.scope_vars.iter().rev() {
                    for (vname, vtype) in scope.iter() {
                        if vname == name {
                            return Some(vtype.clone());
                        }
                    }
                }
                if name == "self" || name == "_cmd" || name == "super" || name == "nil" || name == "NULL" || name == "YES" || name == "NO" || name == "true" || name == "false" {
                    return Some(AstType::new(TypePrim::Int));
                }
                if let Some(ref st) = self.symtab {
                    if st.lookup(name).is_some() {
                        return Some(AstType::new(TypePrim::Int));
                    }
                }
                if let Some(ref cls_name) = self.current_class {
                    if let Some(ref st) = self.symtab {
                        if let Some(cls) = st.find_class(cls_name) {
                            if let SymbolData::Class { ref ivars, .. } = cls.data {
                                if ivars.contains(name) {
                                    return Some(AstType::new(TypePrim::Int));
                                }
                            }
                        }
                    }
                }
                self.check_error(e.line, e.col, &format!("use of undeclared identifier '{}'", name));
                None
            }
            AstExprData::MsgSend { receiver, args, selector, is_class_method, .. } => {
                self.check_expr(&mut *receiver);
                // Receiver kind vs method kind: a class singleton receives only
                // `+` class methods, an instance only `-` instance methods.  In
                // ObjC these are runtime "unrecognized selector" crashes; Nupa
                // (static) rejects them at compile time.
                if let Some(&actual_class) = self.method_kinds.get(selector) {
                    if !actual_class && *is_class_method {
                        self.check_error(e.line, e.col, &format!(
                            "instance method '{}' cannot be called on a class name", selector));
                    } else if actual_class && !*is_class_method {
                        let recv_is_instance = match &receiver.data {
                            AstExprData::VarRef { name, .. } => {
                                name != "self" && name != "_self" && self.is_local_var(name)
                            }
                            AstExprData::IvarRef { obj, .. } => {
                                !matches!(obj.data, AstExprData::MsgSend { .. })
                            }
                            _ => false,
                        };
                        if recv_is_instance {
                            self.check_error(e.line, e.col, &format!(
                                "class method '{}' cannot be called on an instance", selector));
                        }
                    }
                }
                // Check each arg's type against the method's declared parameter
                // types (collected from the AST in `collect_signatures`).
                // Reject bare C-string literals ("...") passed where an object
                // type is expected — only @"..." (AtString) is valid.
                let param_types = self.method_params.get(selector).cloned().unwrap_or_default();
                for a in args.iter_mut() { self.check_expr(a); }
                for (i, a) in args.iter().enumerate() {
                    if let Some(Some(ref pt)) = param_types.get(i) {
                        if Self::is_object_type(pt) && matches!(a.data, AstExprData::String(_)) {
                            self.check_warning(a.line, a.col,
                                "argument as a bare C string is not an object; use @\"...\" for an NPString");
                        }
                    }
                }
                // ARC mode: forbid manual retain/release/dealloc/autorelease
                // unless inside @noarc { } or implementing the runtime method itself.
                if !self.no_arc && !self.in_noarc {
                    let sel = selector.trim_end_matches(':');
                    if sel == "retain" || sel == "release" || sel == "autorelease" || sel == "dealloc" {
                        let impl_ok = self.current_method.as_deref().map(|m| m.trim_end_matches(':') == sel).unwrap_or(false);
                        if !impl_ok {
                            self.check_error(e.line, e.col, &format!(
                                "explicit '{}' not allowed in ARC mode; wrap in @noarc {{ }} to manage manually", sel));
                        }
                    }
                }
                Some(AstType::new(TypePrim::Id))
            }
            AstExprData::FuncCall { name, args, .. } => {
                if name == "NPLog" {
                    if let Some(first) = args.first() {
                        if !matches!(first.data, AstExprData::AtString(_)) {
                            self.check_warning(first.line, first.col, "NPLog first argument should be an NPString literal (@\"...\")");
                        }
                    }
                }
                // General: warn about bare C-string literals passed to object-typed params.
                let param_types = self.function_params.get(name).cloned().unwrap_or_default();
                for a in args.iter_mut() { self.check_expr(a); }
                for (i, a) in args.iter().enumerate() {
                    if let Some(Some(ref pt)) = param_types.get(i) {
                        if Self::is_object_type(pt) && matches!(a.data, AstExprData::String(_)) {
                            self.check_warning(a.line, a.col,
                                "argument as a bare C string is not an object; use @\"...\" for an NPString");
                        }
                    }
                }
                Some(AstType::new(TypePrim::Int))
            }
            AstExprData::Binary { left, right, .. } => {
                self.check_expr(&mut *left);
                self.check_expr(&mut *right);
                Some(AstType::new(TypePrim::Int))
            }
            AstExprData::Unary { operand, .. } => {
                self.check_expr(&mut *operand.clone())
            }
            AstExprData::Assign { target, .. } => {
                self.check_expr(&mut *target.clone())
            }
            AstExprData::Cast { target_type, .. } => {
                Some(target_type.clone())
            }
AstExprData::Subscript { object, key, .. } => {
                self.check_expr(&mut *object);
                self.check_expr(&mut *key);
                Some(AstType::new(TypePrim::Int))
            }
            AstExprData::Ternary { then, .. } => {
                self.check_expr(&mut *then.clone())
            }
            AstExprData::Comma(exprs) => {
                exprs.last().and_then(|e| {
                    let mut e = e.clone();
                    self.check_expr(&mut e)
                })
            }
            AstExprData::IvarRef { obj, .. } => {
                self.check_expr(&mut *obj);
                Some(AstType::new(TypePrim::Int))
            }
            AstExprData::PropRef { obj, .. } => {
                self.check_expr(&mut *obj);
                Some(AstType::new(TypePrim::Int))
            }
            AstExprData::Selector(_) => {
                Some(AstType::new(TypePrim::Sel))
            }
            AstExprData::ArrayLit(_) => Some(AstType::new(TypePrim::Id)),
            AstExprData::InitList(items) => {
                if let Some(first) = items.first() {
                    let mut e = first.clone();
                    self.check_expr(&mut e)
                } else {
                    Some(AstType::new(TypePrim::Int))
                }
            }
            AstExprData::DictLit { .. } => Some(AstType::new(TypePrim::Id)),
            AstExprData::Block { .. } => Some(AstType::new(TypePrim::Id)),
            AstExprData::Sizeof { .. } => {
                let mut t = AstType::new(TypePrim::Long);
                t.is_pointer = false;
                Some(t)
            }
            AstExprData::Alignof(_) => {
                let mut t = AstType::new(TypePrim::Long);
                t.is_pointer = false;
                Some(t)
            }
            AstExprData::TypeLiteral(_) => Some(AstType::new(TypePrim::Int)),
        }
    }

    /// Check a statement
    pub fn check_stmt(&mut self, s: &mut AstStmt) {
        match &mut s.data {
            AstStmtData::Expr(e) => { self.check_expr(e); }
            AstStmtData::Compound(stmts) => {
                self.scope_vars.push(Vec::new());
                for stmt in stmts { self.check_stmt(stmt); }
                self.scope_vars.pop();
            }
            AstStmtData::Return(expr) => {
                if let Some(e) = expr { self.check_expr(&mut *e); }
            }
            AstStmtData::If { cond, then, else_ } => {
                self.check_expr(&mut *cond);
                self.check_stmt(&mut *then);
                if let Some(ref mut els) = else_ { self.check_stmt(&mut *els); }
            }
            AstStmtData::While { cond, body } => {
                self.check_expr(&mut *cond);
                self.check_stmt(&mut *body);
            }
            AstStmtData::Do { body, cond } => {
                self.check_stmt(&mut *body);
                self.check_expr(&mut *cond);
            }
            AstStmtData::For { init, cond, incr, body } => {
                if let Some(ref mut i) = init { self.check_stmt(&mut *i); }
                if let Some(ref mut c) = cond { self.check_expr(&mut *c); }
                if let Some(ref mut i) = incr { self.check_expr(&mut *i); }
                self.check_stmt(&mut *body);
            }
            AstStmtData::ForIn { var, collection, body } => {
                self.check_expr(&mut *var);
                self.check_expr(&mut *collection);
                self.check_stmt(&mut *body);
            }
            AstStmtData::Switch { expr, body } => {
                self.check_expr(&mut *expr);
                self.check_stmt(&mut *body);
            }
            AstStmtData::Case { value, body } => {
                self.check_expr(&mut *value);
                self.check_stmt(&mut *body);
            }
            AstStmtData::Default(body) => { self.check_stmt(&mut *body); }
            AstStmtData::Throw(expr) => {
                if let Some(e) = expr { self.check_expr(&mut *e); }
            }
            AstStmtData::Try { try_block, catches, finally_block } => {
                self.check_stmt(&mut *try_block);
                for c in catches { self.check_stmt(c); }
                if let Some(ref mut f) = finally_block { self.check_stmt(&mut *f); }
            }
            AstStmtData::Catch { body, .. } => { self.check_stmt(&mut *body); }
            AstStmtData::Finally(body) => { self.check_stmt(&mut *body); }
            AstStmtData::Synchronized { lock, body } => {
                self.check_expr(&mut *lock);
                self.check_stmt(&mut *body);
            }
            AstStmtData::Autoreleasepool(body) => { self.check_stmt(&mut *body); }
            AstStmtData::NoArc(body) => {
                let old = self.in_noarc;
                self.in_noarc = true;
                self.check_stmt(&mut *body);
                self.in_noarc = old;
            }
            AstStmtData::Decl(d) => { self.check_decl(d); }
            _ => {}
        }
    }

    /// Check a declaration
    fn add_params_to_scope(&mut self, params: &Option<Box<CstParam>>) {
        if self.scope_vars.is_empty() {
            self.scope_vars.push(Vec::new());
        }
        let mut p = params.as_ref().map(|b| &**b);
        while let Some(param) = p {
            if let Some(ref name) = param.name {
                if let Some(scope) = self.scope_vars.last_mut() {
                    if !scope.iter().any(|(n, _)| n == name) {
                        let t = param.par_type.as_ref()
                            .map(|ct| Self::cst_type_to_ast_type(ct))
                            .unwrap_or_else(|| AstType::new(TypePrim::Int));
                        scope.push((name.clone(), t));
                    }
                }
            }
            p = param.next.as_ref().map(|n| &**n);
        }
    }

    pub fn check_decl(&mut self, d: &mut AstDecl) {
        match &mut d.data {
            AstDeclData::Function { body, params, .. } => {
                if let Some(ref mut b) = body {
                    let old = self.current_method.clone();
                    self.current_method = d.name.clone();
                    self.add_params_to_scope(params);
                    self.check_stmt(b);
                    self.current_method = old;
                }
            }
            AstDeclData::Variable { var_type, init, .. } => {
                if let Some(ref name) = d.name {
                    if let Some(scope) = self.scope_vars.last_mut() {
                        if !scope.iter().any(|(n, _)| n == name) {
                            let t = var_type.as_ref().map(|b| *b.clone()).unwrap_or_else(|| AstType::new(TypePrim::Int));
                            scope.push((name.clone(), t));
                        }
                    }
                }
                if let Some(ref mut i) = init { self.check_expr(i); }
            }
            AstDeclData::Class { methods, .. } => {
                let old = self.current_class.clone();
                self.current_class = d.name.clone();
                for m in methods { self.check_decl(m); }
                self.current_class = old;
            }
            AstDeclData::Method { body, params, .. } => {
                if let Some(ref mut b) = body {
                    let old = self.current_method.clone();
                    self.current_method = d.name.clone();
                    self.add_params_to_scope(params);
                    self.check_stmt(b);
                    self.current_method = old;
                }
            }
            _ => {}
        }
    }

    /// Check the entire AST unit
    pub fn check(&mut self, unit: &mut AstUnit) -> i32 {
        // First pass: collect method/function signatures from the AST.
        for decl in &unit.decls {
            self.collect_signatures(decl);
        }
        for decl in &mut unit.decls {
            self.check_decl(decl);
        }
        if self.has_error { -1 } else { 0 }
    }

    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }
}