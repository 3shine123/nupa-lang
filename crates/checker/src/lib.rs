use nupa_ast::ast::*;
use nupa_cst::TypePrim;
use nupa_symbol::*;

/// Type checker for Nupa programs.
/// Validates types and reports type errors.
pub struct Checker {
    pub symtab: Option<SymbolTable>,
    pub current_class: Option<String>,
    pub current_method: Option<String>,
    pub has_error: bool,
    pub error_count: i32,
    pub error_msg: String,
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
        self.error_msg = msg.to_string();
        eprintln!("type error:{}:{}: {}", line, col, msg);
    }

    fn is_numeric_type(prim: TypePrim) -> bool {
        matches!(prim, TypePrim::Char | TypePrim::Short | TypePrim::Int | TypePrim::Long
            | TypePrim::LongLong | TypePrim::Float | TypePrim::Double | TypePrim::Bool
            | TypePrim::Signed | TypePrim::Unsigned)
    }

    /// Check an expression and return its type
    pub fn check_expr(&mut self, e: &AstExpr) -> Option<AstType> {
        match &e.data {
            AstExprData::Int(_) => Some(AstType::new(TypePrim::Int)),
            AstExprData::Float(_) => Some(AstType::new(TypePrim::Double)),
            AstExprData::String(_) => {
                let mut t = AstType::new(TypePrim::Char);
                t.is_pointer = true;
                Some(t)
            }
            AstExprData::Char(_) => Some(AstType::new(TypePrim::Char)),
            AstExprData::Bool(_) => Some(AstType::new(TypePrim::Bool)),
            AstExprData::VarRef { name, .. } => {
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
            AstExprData::MsgSend { .. } => {
                Some(AstType::new(TypePrim::Id))
            }
            AstExprData::FuncCall { .. } => {
                Some(AstType::new(TypePrim::Int))
            }
            AstExprData::Binary { .. } => {
                Some(AstType::new(TypePrim::Int))
            }
            AstExprData::Unary { operand, .. } => {
                self.check_expr(operand)
            }
            AstExprData::Assign { target, .. } => {
                self.check_expr(target)
            }
            AstExprData::Cast { target_type, .. } => {
                Some(target_type.clone())
            }
            AstExprData::Subscript { object, .. } => {
                self.check_expr(object)
            }
            AstExprData::Ternary { then, .. } => {
                self.check_expr(then)
            }
            AstExprData::Comma(exprs) => {
                exprs.last().and_then(|e| self.check_expr(e))
            }
            AstExprData::IvarRef { .. } => {
                Some(AstType::new(TypePrim::Int))
            }
            AstExprData::PropRef { .. } => {
                Some(AstType::new(TypePrim::Int))
            }
            AstExprData::Selector(_) => {
                Some(AstType::new(TypePrim::Sel))
            }
            AstExprData::ArrayLit(items) => {
                if let Some(first) = items.first() {
                    self.check_expr(first)
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
        }
    }

    /// Check a statement
    pub fn check_stmt(&mut self, s: &AstStmt) {
        match &s.data {
            AstStmtData::Expr(e) => { self.check_expr(e); }
            AstStmtData::Compound(stmts) => {
                for stmt in stmts { self.check_stmt(stmt); }
            }
            AstStmtData::Return(expr) => {
                if let Some(e) = expr { self.check_expr(e); }
            }
            AstStmtData::If { cond, then, else_ } => {
                self.check_expr(cond);
                self.check_stmt(then);
                if let Some(ref els) = else_ { self.check_stmt(els); }
            }
            AstStmtData::While { cond, body } => {
                self.check_expr(cond);
                self.check_stmt(body);
            }
            AstStmtData::Do { body, cond } => {
                self.check_stmt(body);
                self.check_expr(cond);
            }
            AstStmtData::For { init, cond, incr, body } => {
                if let Some(ref i) = init { self.check_stmt(i); }
                if let Some(ref c) = cond { self.check_expr(c); }
                if let Some(ref i) = incr { self.check_expr(i); }
                self.check_stmt(body);
            }
            AstStmtData::ForIn { var, collection, body } => {
                self.check_expr(var);
                self.check_expr(collection);
                self.check_stmt(body);
            }
            AstStmtData::Switch { expr, body } => {
                self.check_expr(expr);
                self.check_stmt(body);
            }
            AstStmtData::Case { value, body } => {
                self.check_expr(value);
                self.check_stmt(body);
            }
            AstStmtData::Default(body) => { self.check_stmt(body); }
            AstStmtData::Throw(expr) => {
                if let Some(e) = expr { self.check_expr(e); }
            }
            AstStmtData::Try { try_block, catches, finally_block } => {
                self.check_stmt(try_block);
                for c in catches { self.check_stmt(c); }
                if let Some(ref f) = finally_block { self.check_stmt(f); }
            }
            AstStmtData::Catch { body, .. } => { self.check_stmt(body); }
            AstStmtData::Finally(body) => { self.check_stmt(body); }
            AstStmtData::Synchronized { body, .. } => { self.check_stmt(body); }
            AstStmtData::Autoreleasepool(body) => { self.check_stmt(body); }
            AstStmtData::Decl(d) => { self.check_decl(d); }
            _ => {}
        }
    }

    /// Check a declaration
    pub fn check_decl(&mut self, d: &AstDecl) {
        match &d.data {
            AstDeclData::Function { body, .. } => {
                if let Some(ref b) = body {
                    let old = self.current_method.clone();
                    self.current_method = d.name.clone();
                    self.check_stmt(b);
                    self.current_method = old;
                }
            }
            AstDeclData::Variable { init, .. } => {
                if let Some(ref i) = init { self.check_expr(i); }
            }
            AstDeclData::Class { methods, .. } => {
                let old = self.current_class.clone();
                self.current_class = d.name.clone();
                for m in methods { self.check_decl(m); }
                self.current_class = old;
            }
            AstDeclData::Method { body, .. } => {
                if let Some(ref b) = body {
                    let old = self.current_method.clone();
                    self.current_method = d.name.clone();
                    self.check_stmt(b);
                    self.current_method = old;
                }
            }
            _ => {}
        }
    }

    /// Check the entire AST unit
    pub fn check(&mut self, unit: &AstUnit) -> i32 {
        for decl in &unit.decls {
            self.check_decl(decl);
        }
        if self.has_error { -1 } else { 0 }
    }
}