use std::fmt::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use nupa_ast::*;
use nupa_symbol::*;
use nupa_cst::TypePrim;

// ─── Temp variable counter ─────────────────────────────────────────────────
static TEMP_VAR_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn next_temp_id() -> usize {
    TEMP_VAR_COUNTER.fetch_add(1, Ordering::SeqCst)
}

// ─── Block literal data ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BlockLiteralData {
    pub return_type: String,
    pub params: Vec<(String, String)>,  // (type, name)
    pub body: Option<Box<CgStmt>>,
    pub func_name: String,
}

fn fnv1a_hash(s: &str) -> u32 {
    let mut hash: u32 = 0x811C9DC5;
    for b in s.bytes() {
        hash ^= b as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}

fn sanitize_sel_name(sel: &str) -> String {
    sel.replace(':', "_")
}

fn sel_const_name(sel: &str) -> String {
    format!("__nupa_sel_{}", sanitize_sel_name(sel))
}

// ─── C99 AST types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum CgExprKind {
    Int, Float, String, Char, Ident, Sizeof,
    Unary, Binary, Assign, Cast,
    Call, Comma, Member, Arrow, Index, Ternary,
    InitList, BlockLit,
}

#[derive(Debug, Clone)]
pub struct CgExpr {
    pub kind: CgExprKind,
    pub type_str: Option<String>,
    pub line: usize, pub col: usize,
    pub data: CgExprData,
}

#[derive(Debug, Clone)]
pub enum CgExprData {
    Int(i64), Float(f64), String(String), Char(u8), Ident(String),
    Unary { op_str: String, operand: Box<CgExpr>, is_postfix: bool },
    Binary { op_str: String, left: Box<CgExpr>, right: Box<CgExpr> },
    Assign { target: Box<CgExpr>, value: Box<CgExpr> },
    Cast { target_type: String, expr: Box<CgExpr> },
    Call {
        name: String, args: Vec<CgExpr>,
        vtable_class: Option<String>,
        alt_vtable_classes: Vec<String>,
        is_class_method: bool, is_super: bool,
        sel_const_name: Option<String>,
    },
    Comma(Vec<CgExpr>),
    Member { obj: Box<CgExpr>, field: String },
    Arrow { obj: Box<CgExpr>, field: String },
    Index { arr: Box<CgExpr>, index: Box<CgExpr> },
    Ternary { cond: Box<CgExpr>, then: Box<CgExpr>, else_: Box<CgExpr> },
    InitList(Vec<CgExpr>),
    BlockLit(BlockLiteralData),
    Sizeof(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CgStmtKind {
    Empty, Expr, Compound, If, Switch, Case, Default,
    While, Do, For, ForIn,
    Break, Continue, Return, Goto, Label, Decl,
}

#[derive(Debug, Clone)]
pub struct CgStmt {
    pub kind: CgStmtKind,
    pub line: usize, pub col: usize,
    pub data: CgStmtData,
}

#[derive(Debug, Clone)]
pub enum CgStmtData {
    Expr(CgExpr),
    Compound(Vec<CgStmt>),
    If { cond: Box<CgExpr>, then: Box<CgStmt>, else_: Option<Box<CgStmt>> },
    Switch { expr: Box<CgExpr>, body: Box<CgStmt> },
    Case { value: Box<CgExpr>, body: Box<CgStmt> },
    Default(Box<CgStmt>),
    While { cond: Box<CgExpr>, body: Box<CgStmt> },
    Do { body: Box<CgStmt>, cond: Box<CgExpr> },
    For { init: Option<Box<CgStmt>>, cond: Option<Box<CgExpr>>, incr: Option<Box<CgExpr>>, body: Box<CgStmt> },
    ForIn { var_name: String, collection: Box<CgExpr>, body: Box<CgStmt> },
    Return(Option<Box<CgExpr>>),
    Goto(String),
    Label(String),
    Decl { decl_type: String, name: String, init: Option<Box<CgExpr>>, array_suffix: Option<String>, is_static: bool, is_weak: bool, next: Vec<(String, Option<Box<CgExpr>>)> },
    Break,
    Continue,
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CgDeclKind {
    Function, Variable, Typedef, Struct, ExternFunc, Enum,
}

#[derive(Debug, Clone)]
pub struct CgDecl {
    pub kind: CgDeclKind,
    pub name: String,
    pub data: CgDeclData,
}

#[derive(Debug, Clone)]
pub enum CgDeclData {
    Function {
        return_type: String,
        params: Vec<(String, String)>,
        is_variadic: bool,
        is_objc_class: bool,
        body: Option<Box<CgStmt>>,
    },
    Variable {
        var_type: String,
        init: Option<Box<CgExpr>>,
        is_static: bool,
        is_const: bool,
        is_weak: bool,
        next: Vec<(String, Option<Box<CgExpr>>)>,
    },
    Typedef {
        alias: String,
        type_str: String,
        struct_fields: Vec<(String, String)>,
    },
    Struct { fields: Vec<(String, String)> },
    ExternFunc {
        return_type: String,
        params: Vec<(String, String)>,
        is_variadic: bool,
    },
    Enum { members: Vec<(String, String)> },
}

#[derive(Debug, Clone)]
pub struct CgUnit {
    pub decls: Vec<CgDecl>,
    pub filename: String,
    pub c_headers: Vec<String>,
    pub selectors: Vec<String>,
    pub classes: Vec<CgClassMeta>,
}

#[derive(Debug, Clone)]
pub struct CgClassMeta {
    pub class_name: String,
    pub super_name: Option<String>,
    pub method_names: Vec<String>,
    pub is_class_methods: Vec<bool>,
    pub method_return_types: Vec<String>,
    pub method_params_list: Vec<Vec<(String, String)>>,
    pub method_owners: Vec<String>,
    pub vtable_indices: Vec<i32>,
    pub ivar_types: Vec<String>,
    pub ivar_names: Vec<String>,
    pub properties: Vec<String>,
}

// ─── AST Type → C string ─────────────────────────────────────────────────────

fn name_flat(fqn: &str) -> String {
    // First mangle generic type arguments: `Name<T1, T2*>` → `Name_T1_T2_ptr`
    // so that e.g. `DataPack<QuantumToken*>` becomes `DataPack_QuantumToken_ptr`.
    // This is used for vtable/class metadata symbols emitted per instantiation.
    let mut out = String::new();
    let mut depth = 0; // inside <...>?
    let mut cur_arg = String::new();
    let mut base = String::new();
    let mut in_args = false;
    for ch in fqn.chars() {
        if !in_args {
            if ch == '<' {
                in_args = true;
                depth = 1;
            } else {
                base.push(ch);
            }
            continue;
        }
        // inside <...>
        if ch == '<' { depth += 1; cur_arg.push(ch); continue; }
        if ch == '>' {
            depth -= 1;
            if depth == 0 {
                // close this arg group
                let mangled = mangle_one_arg(&cur_arg);
                out.push_str(&mangled);
                cur_arg.clear();
                in_args = false;
                continue;
            }
            cur_arg.push(ch);
            continue;
        }
        if ch == ',' && depth == 1 {
            let mangled = mangle_one_arg(&cur_arg);
            out.push('_');
            out.push_str(&mangled);
            cur_arg.clear();
        } else {
            cur_arg.push(ch);
        }
    }
    // Build final: base + "_" + args if any args were rendered
    if out.is_empty() {
        // No generic args — just :: replacement
        return base.replace("::", "__");
    }
    // Insert separator between base and first arg
    let mut full = base.replace("::", "__");
    full.push('_');
    full.push_str(&out);
    full
}

// Mangle a single generic type argument like `QuantumToken *` → `QuantumToken_ptr`.
fn mangle_one_arg(arg: &str) -> String {
    let trimmed = arg.trim();
    let mut s = String::new();
    // strip qualifiers we don't want in mangled name
    let cleaned = trimmed.replace("const ", "").replace("volatile ", "");
    let mut iter = cleaned.chars().peekable();
    let mut name = String::new();
    let mut ptr_count = 0;
    // collect identifier name (may include ::)
    while let Some(&c) = iter.peek() {
        if c.is_alphanumeric() || c == '_' || c == ':' {
            name.push(c);
            iter.next();
        } else {
            break;
        }
    }
    // count trailing `*`
    while let Some(&c) = iter.peek() {
        if c == '*' { ptr_count += 1; iter.next(); }
        else if c.is_whitespace() { iter.next(); }
        else { break; }
    }
    let flat = name.replace("::", "__");
    s.push_str(&flat);
    for _ in 0..ptr_count { s.push_str("_ptr"); }
    s
}

fn cst_type_to_c_str(ct: &nupa_cst::CstType) -> String {
    if ct.is_block {
        let ret = ct.subtype.as_ref().map(|s| cst_type_to_c_str(s)).unwrap_or_else(|| "void".into());
        let mut params = String::new();
        let mut bp = ct.block_params.as_ref();
        while let Some(b) = bp {
            if !params.is_empty() { params.push_str(", "); }
            params.push_str(&cst_type_to_c_str(b));
            bp = b.next.as_ref();
        }
        if params.is_empty() { params.push_str("void"); }
        // Block types: emit as pointer to function type (void (^)() → void (*)())
        if ct.is_pointer {
            return format!("{} (*)({})", ret, params);
        }
        return format!("{} (^)({})", ret, params);
    }
    if ct.is_array {
        let base = ct.subtype.as_ref().map(|s| cst_type_to_c_str(s)).unwrap_or_else(|| "int".into());
        if ct.array_size > 0 { return format!("{}[{}]", base, ct.array_size); }
        return format!("{}[]", base);
    }
    let mut s = String::new();
    // For pointer types, const on the pointed-to type (subtype) goes BEFORE the base type
    if ct.is_pointer {
        if ct.subtype.as_ref().map(|st| st.is_const).unwrap_or(false) {
            s.push_str("const ");
        }
    } else {
        // For non-pointer types, const applies to the type itself
        if ct.is_const { s.push_str("const "); }
    }
    match ct.prim {
        TypePrim::Void => s.push_str("void"),
        TypePrim::Char => s.push_str("char"),
        TypePrim::Short => s.push_str("short"),
        TypePrim::Int => s.push_str("int"),
        TypePrim::Long => s.push_str("long"),
        TypePrim::LongLong => s.push_str("long long"),
        TypePrim::Float => s.push_str("float"),
        TypePrim::Double => s.push_str("double"),
        TypePrim::Bool => s.push_str("_Bool"),
        TypePrim::Signed => s.push_str("signed"),
        TypePrim::Unsigned => s.push_str("unsigned"),
        TypePrim::Id => s.push_str("NPObject *"),
        TypePrim::Class => s.push_str("Class"),
        TypePrim::Sel => s.push_str("SEL"),
        TypePrim::Instancetype => s.push_str("NPObject *"),
        TypePrim::Param => s.push_str("NPObject *"),
        TypePrim::Named => {
            if let Some(ref name) = ct.name {
                if ct.is_struct { s.push_str(&format!("struct {}", name)); }
                else { s.push_str(&name_flat(name)); }
            } else { s.push_str("int"); }
        }
    }
    if ct.is_pointer {
        s.push_str(" *");
        // const on the pointer itself goes AFTER the *
        if ct.is_const {
            s.push_str("const ");
        }
    }
    s
}

pub fn ast_type_to_c_str(t: &AstType) -> String {
    if t.is_block {
        let ret = t.subtype.as_ref().map(|s| ast_type_to_c_str(s)).unwrap_or_else(|| "void".into());
        let mut params = String::new();
        let mut bp = t.block_params.as_ref();
        while let Some(b) = bp {
            if !params.is_empty() { params.push_str(", "); }
            params.push_str(&ast_type_to_c_str(b));
            bp = b.next.as_ref();
        }
        if params.is_empty() { params.push_str("void"); }
        if let Some(ref bn) = t.block_name {
            return format!("{} (^{})({})", ret, bn, params);
        }
        return format!("{} (^)({})", ret, params);
    }
    if t.is_array {
        let base = t.subtype.as_ref().map(|s| ast_type_to_c_str(s)).unwrap_or_else(|| "int".into());
        if t.array_size > 0 { return format!("{}[{}]", base, t.array_size); }
        // Symbolic size (e.g. `MAX_CHILDREN`) — emit the named constant
        // so the field is a fixed-size array, NOT a flexible array member
        // (C forbids FAMs outside the trailing field).
        if let Some(ref name) = t.array_size_name {
            return format!("{}[{}]", base, name);
        }
        return format!("{}[]", base);
    }
    let mut s = String::new();

    // For pointer types, const on the pointed-to type (subtype) goes BEFORE the base type
    if t.is_pointer {
        if t.subtype.as_ref().map(|st| st.is_const).unwrap_or(false) {
            s.push_str("const ");
        }
    } else {
        // For non-pointer types, const applies to the type itself
        if t.is_const { s.push_str("const "); }
    }

    match t.prim {
        TypePrim::Void => s.push_str("void"),
        TypePrim::Char => s.push_str("char"),
        TypePrim::Short => s.push_str("short"),
        TypePrim::Int => s.push_str("int"),
        TypePrim::Long => s.push_str("long"),
        TypePrim::LongLong => s.push_str("long long"),
        TypePrim::Float => s.push_str("float"),
        TypePrim::Double => s.push_str("double"),
        TypePrim::Bool => s.push_str("_Bool"),
        TypePrim::Signed => s.push_str("signed"),
        TypePrim::Unsigned => s.push_str("unsigned"),
        TypePrim::Id => s.push_str("NPObject *"),
        TypePrim::Class => s.push_str("Class"),
        TypePrim::Sel => s.push_str("SEL"),
        TypePrim::Instancetype => s.push_str("NPObject *"),
        TypePrim::Param => s.push_str("NPObject *"),
        TypePrim::Named => {
            if let Some(ref name) = t.name {
                // Use class_ref if available (resolved by elaborator), otherwise use name
                let type_name = if let Some(ref cr) = t.class_ref {
                    cr
                } else {
                    name
                };
                let flat = name_flat(type_name);
                if t.is_struct {
                    s.push_str(&format!("struct {}", flat));
                } else {
                    s.push_str(&flat);
                }
            } else {
                s.push_str("int");
            }
        }
    }

    if t.is_pointer {
        // For multi-level pointers (e.g. `int **`), the subtype is itself a
        // pointer. Emit each `*` recursively so `int** _rawGrid;` becomes
        // `int * * _rawGrid;` (not `int * _rawGrid;`, which truncates the
        // extra indirection and breaks malloc casts + subscripting).
        if let Some(ref sub) = t.subtype {
            if sub.is_pointer {
                s.push_str(" *");
                // Render the remaining pointer levels from the subtype chain.
                let mut cur = sub.clone();
                while cur.is_pointer {
                    s.push_str(" *");
                    if let Some(ref deeper) = cur.subtype {
                        cur = deeper.clone();
                    } else {
                        break;
                    }
                }
                // const on the outermost pointer goes AFTER the final *
                if t.is_const { s.push_str(" const"); }
                return s;
            }
        }
        s.push_str(" *");
        // const on the pointer itself goes AFTER the *
        if t.is_const {
            s.push_str("const ");
        }
    }

    // Generic type args
    if !t.type_args.is_empty() {
        // Just use the base name for generics
    }

    s
}

// ─── AST → CG conversion ─────────────────────────────────────────────────────

// All ObjC class instances are emitted as `Type *` (pointer to struct) in the
// generated C. When the user writes `obj.field` (source `.`) on such an
// instance, the elaborator's fallback path preserves `is_arrow=false`, which
// would emit a C `.` — wrong, since the instance is a pointer. This helper
// inspects the object expression of a PropRef and returns true when we can
// prove the object is an ObjC class instance (forcing `->`).
fn objc_instance_needs_arrow(obj: &AstExpr, class_infos: &std::collections::BTreeMap<String, ClassInfo>) -> bool {
    // Self/super inside a method body: instances are `Type *` → `->`.
    if matches!(obj.kind, AstExprKind::Self_ | AstExprKind::Super) {
        return true;
    }
    // A VarRef whose name matches a known ObjC class is a class-method
    // receiver; not an instance. Skip.
    if let AstExprData::VarRef { name, .. } = &obj.data {
        // If the identifier is itself a registered class name, this is a
        // class-method receiver (`[Student alloc]`), not an instance — `.` is fine.
        if class_infos.values().any(|ci| ci.class_name == *name) {
            return false;
        }
        // Otherwise, if the identifier resolves to an ivar of some class,
        // the ivar's type might be a class instance. We can't easily tell
        // here without symtab, so be conservative: only force `->` when we
        // know the object is a class instance. Fall through to default below.
    }
    // Cast expressions like `((struct Student *)self)` always produce a
    // pointer; force `->`.
    if matches!(obj.kind, AstExprKind::Cast) {
        return true;
    }
    false
}

fn op_to_str(op: i32, is_assign: bool) -> &'static str {
    if is_assign {
        match op {
            0 => "=", 1 => "+=", 2 => "-=", 3 => "*=", 4 => "/=", 5 => "%=",
            6 => "&=", 7 => "|=", 8 => "^=", 9 => "<<=", 10 => ">>=",
            _ => "=",
        }
    } else {
        match op {
            1 => "*", 2 => "/", 3 => "%", 4 => "+", 5 => "-",
            6 => "<<", 7 => ">>", 8 => "<", 9 => ">", 10 => "<=", 11 => ">=",
            12 => "==", 13 => "!=", 14 => "&", 15 => "^", 16 => "|",
            17 => "&&", 18 => "||",
            // Compound assignment operators (used in AstExprData::Binary when is_assign is not tracked)
            100 => "+=", 101 => "-=", 102 => "*=", 103 => "/=", 104 => "%=",
            105 => "&=", 106 => "|=", 107 => "^=", 108 => "<<=", 109 => ">>=",
            _ => "?",
        }
    }
}

#[derive(Debug, Clone)]
struct ClassInfo {
    class_name: String,
    flat: String,
    super_name: Option<String>,
    method_names: Vec<String>,
    is_class_methods: Vec<bool>,
    method_bodies: Vec<Option<Box<CgStmt>>>,
    method_return_types: Vec<String>,
    method_params_list: Vec<Vec<(String, String)>>,
    method_owners: Vec<String>,
    ivar_types: Vec<String>,
    ivar_names: Vec<String>,
}

// Render an AstExpr callee to a C expression string (used for block invocation on ivars)
fn render_callee_expr(ae: &AstExpr) -> String {
    match &ae.data {
        AstExprData::IvarRef { obj, ivar, .. } => {
            let obj_str = render_callee_expr(obj);
            if let Some(ref iv) = ivar {
                format!("{}->{}", obj_str, iv)
            } else {
                obj_str
            }
        }
        AstExprData::VarRef { name, .. } => {
            if ae.kind == AstExprKind::Self_ || ae.kind == AstExprKind::Super {
                "_self".to_string()
            } else {
                name.clone()
            }
        }
        _ => String::new(),
    }
}

fn convert_expr(ae: &AstExpr, class_infos: &std::collections::BTreeMap<String, ClassInfo>) -> CgExpr {
    let line = ae.line; let col = ae.col;
    let type_str = ae.expr_type.as_ref().map(|t| ast_type_to_c_str(t));
    // Handle kind-based matching for variants not in AstExprData
    if ae.kind == AstExprKind::Nil {
        return CgExpr { kind: CgExprKind::Ident, type_str, line, col, data: CgExprData::Ident("NULL".into()) };
    }
    if ae.kind == AstExprKind::Null {
        return CgExpr { kind: CgExprKind::Ident, type_str, line, col, data: CgExprData::Ident("NULL".into()) };
    }
    if ae.kind == AstExprKind::Self_ {
        return CgExpr { kind: CgExprKind::Ident, type_str, line, col, data: CgExprData::Ident("self".into()) };
    }
    if ae.kind == AstExprKind::Super {
        return CgExpr { kind: CgExprKind::Ident, type_str, line, col, data: CgExprData::Ident("super".into()) };
    }
    if ae.kind == AstExprKind::BlockLit {
        // BlockLit is handled in the match below via AstExprData::Block
    }
    match &ae.data {
        AstExprData::Int(val) => CgExpr { kind: CgExprKind::Int, type_str, line, col, data: CgExprData::Int(*val) },
        AstExprData::Float(val) => CgExpr { kind: CgExprKind::Float, type_str, line, col, data: CgExprData::Float(*val) },
        AstExprData::String(s) => CgExpr { kind: CgExprKind::String, type_str, line, col, data: CgExprData::String(s.clone()) },
        AstExprData::Char(val) => CgExpr { kind: CgExprKind::Char, type_str, line, col, data: CgExprData::Char(*val) },
        AstExprData::Bool(val) => CgExpr { kind: CgExprKind::Int, type_str, line, col, data: CgExprData::Int(if *val { 1 } else { 0 }) },
        AstExprData::VarRef { name, .. } => CgExpr { kind: CgExprKind::Ident, type_str, line, col, data: CgExprData::Ident(name.clone()) },
        AstExprData::IvarRef { ivar, obj, .. } => {
            let mut obj_cg = convert_expr(obj, &class_infos);
            // Use _self for ivar access (method body declares _self as casted struct)
            if let CgExprData::Ident(ref name) = obj_cg.data {
                if name == "self" {
                    obj_cg.data = CgExprData::Ident("_self".into());
                }
            }
            let field = ivar.clone().unwrap_or_default();
            CgExpr { kind: CgExprKind::Arrow, type_str, line, col, data: CgExprData::Arrow { obj: Box::new(obj_cg), field } }
        }
        AstExprData::MsgSend { receiver, selector, args, is_class_method, is_super, super_name, .. } => {
            let mut call_args = Vec::new();
            let mut effective_is_class = *is_class_method;

            let mut vtable_class = None;
            let mut alt_vtable_classes: Vec<String> = Vec::new();
            if *is_super {
                // For super calls, start with the direct superclass name.
                // Walk the superclass chain to find the nearest ancestor that
                // actually declares the method (the direct superclass may inherit
                // it — e.g. NPLayer4 doesn't have dealloc, only NPObject does).
                vtable_class = super_name.clone();
                if let Some(vc) = vtable_class.clone() {
                    let sel = sanitize_sel_name(selector);
                    let mut current = vc;
                    loop {
                        let flat = name_flat(&current);
                        let has = class_infos.get(&flat).map_or(false, |info| {
                            info.method_names.iter().any(|n| n == &sel)
                        });
                        if has { vtable_class = Some(current); break; }
                        match class_infos.get(&flat).and_then(|info| info.super_name.clone()) {
                            Some(sup) => current = sup,
                            None => break,
                        }
                    }
                }
                if vtable_class.is_none() {
                    // super_name not set — fallback: find any class that has the method
                    let sel = sanitize_sel_name(selector);
                    let mut best = None;
                    for (_, info) in class_infos.iter() {
                        if let Some(idx) = info.method_names.iter().position(|n| n == &sel) {
                            if let Some(owner) = info.method_owners.get(idx) {
                                if let Some(owner_info) = class_infos.get(owner) {
                                    if let Some(ref sup) = owner_info.super_name {
                                        if sup != "NPObject" {
                                            best = Some(sup.clone());
                                            break;
                                        }
                                        best = Some(sup.clone());
                                    }
                                }
                            }
                        }
                    }
                    vtable_class = best;
                }
            } else {
                vtable_class = None;
                for (_, info) in class_infos.iter() {
                    if let Some(idx) = info.method_names.iter().position(|n| n == &sanitize_sel_name(selector)) {
                        let cls_name = info.class_name.clone();
                        if vtable_class.is_none() {
                            vtable_class = Some(cls_name);
                            effective_is_class = info.is_class_methods[idx];
                        } else {
                            alt_vtable_classes.push(cls_name);
                        }
                    }
                }
            }

            if effective_is_class {
                if let Some(ref vc) = vtable_class {
                    let receiver_class = match &receiver.data {
                        AstExprData::VarRef { name, .. } => Some(name.clone()),
                        _ => None,
                    };
                    if let Some(rc) = receiver_class {
                        // For self/super class method calls, pass self directly (it's already a class pointer)
                        let cls_addr = if rc == "self" || rc == "super" {
                            CgExpr {
                                kind: CgExprKind::Ident, type_str: None, line, col,
                                data: CgExprData::Ident(rc),
                            }
                        } else {
                            // Resolve short name to fully-qualified class name via class_infos
                            let fq_rc: &str = {
                                let flat_rc = name_flat(&rc);
                                let mut resolved: Option<&str> = None;
                                if let Some(info) = class_infos.get(&flat_rc) {
                                    resolved = Some(info.class_name.as_str());
                                } else {
                                    // Search by suffix match (short name → FQN)
                                    let search_suffix = format!("::{}", rc);
                                    for (_, info) in class_infos.iter() {
                                        if info.class_name == rc
                                            || info.class_name.ends_with(&search_suffix)
                                            || info.flat == rc
                                            || info.flat.ends_with(&flat_rc)
                                        {
                                            resolved = Some(info.class_name.as_str());
                                            break;
                                        }
                                    }
                                }
                                resolved.unwrap_or(rc.as_str())
                            };
                            CgExpr {
                                kind: CgExprKind::Unary, type_str: None, line, col,
                                data: CgExprData::Unary {
                                    op_str: "&".into(),
                                    operand: Box::new(CgExpr {
                                        kind: CgExprKind::Ident, type_str: None, line, col,
                                        data: CgExprData::Ident(format!("nupa_{}_class", name_flat(fq_rc))),
                                    }),
                                    is_postfix: false,
                                },
                            }
                        };
                        call_args.push(cls_addr);
                    }
                }
            } else {
                // Instance method: receiver is self for super calls
                let receiver_expr = if *is_super {
                    CgExpr { kind: CgExprKind::Ident, type_str: None, line: 0, col: 0, data: CgExprData::Ident("self".into()) }
                } else {
                    convert_expr(receiver, &class_infos)
                };
                call_args.push(receiver_expr);
            }
            for a in args {
                call_args.push(convert_expr(a, &class_infos));
            }

            let sel_const = format!("sel_registerName(\"{}\")", selector);

            CgExpr {
                kind: CgExprKind::Call, type_str, line, col,
                data: CgExprData::Call {
                    name: sanitize_sel_name(selector),
                    args: call_args,
                    vtable_class,
                    alt_vtable_classes,
                    is_class_method: effective_is_class,
                    is_super: *is_super,
                    sel_const_name: Some(sel_const),
                },
            }
        }
        AstExprData::FuncCall { name, args, callee, .. } => {
            // When `callee` is Some (block invocation on ivar), use it as
            // the call target instead of a bare function name.
            let call_name = if let Some(ref ce) = callee {
                render_callee_expr(ce)
            } else {
                name.clone()
            };
            let mut auto_sel = None;
            if callee.is_none() {
                // Detect direct calls to NPObject method functions like
                // `NPObject_release(obj)`, `NPObject_retain(obj)`, `NPObject_dealloc(obj)`.
                auto_sel = if let Some(method) = call_name.strip_prefix("NPObject_") {
                    if !method.is_empty() && args.len() == 1 {
                        Some(format!("sel_registerName(\"{}\")", method))
                    } else { None }
                } else { None };
            }
            let mut cg_args: Vec<CgExpr> = args.iter().map(|a| convert_expr(a, &class_infos)).collect();
            if let Some(sel) = auto_sel {
                cg_args.push(CgExpr {
                    kind: CgExprKind::Ident, type_str: None, line, col,
                    data: CgExprData::Ident(sel),
                });
            }
            CgExpr {
                kind: CgExprKind::Call, type_str, line, col,
                data: CgExprData::Call {
                    name: call_name, args: cg_args,
                    vtable_class: None, alt_vtable_classes: vec![], is_class_method: false, is_super: false,
                    sel_const_name: None,
                },
            }
        }
        AstExprData::Unary { op, operand, is_postfix } => {
            let op_str = match op {
                1 => "++", 2 => "--", 3 => "*", 4 => "&", 5 => "-", 6 => "+", 7 => "~", 8 => "!",
                _ => "?",
            };
            CgExpr { kind: CgExprKind::Unary, type_str, line, col, data: CgExprData::Unary { op_str: op_str.into(), operand: Box::new(convert_expr(operand, &class_infos)), is_postfix: *is_postfix } }
        }
        AstExprData::Binary { op, left, right } => {
            if (100..=109).contains(op) {
                // Compound assignment (`+=`, `-=`, etc.) on ObjC property:
                // obj.prop += value  →  [obj setProp:([obj prop] + value)]
                if let AstExprData::PropRef { obj, name, prop, cls, .. } = &left.data {
                    if prop.is_some() {
                        let regular_op = match *op {
                            100 => 4, 101 => 5, 102 => 1, 103 => 2, 104 => 3,
                            105 => 14, 106 => 16, 107 => 15, 108 => 6, 109 => 7,
                            _ => 4,
                        };
                        let regular_op_str = op_to_str(regular_op, false);
                        let obj_cg = convert_expr(obj, &class_infos);
                        let getter_cg = convert_expr(left, &class_infos);
                        let value_cg = convert_expr(right, &class_infos);
                        let sum_cg = CgExpr {
                            kind: CgExprKind::Binary, type_str: None, line, col,
                            data: CgExprData::Binary {
                                op_str: regular_op_str.into(),
                                left: Box::new(getter_cg),
                                right: Box::new(value_cg),
                            },
                        };
                        let setter_sel = format!("set{}{}:", &name[..1].to_uppercase(), &name[1..]);
                        let vtable_class = cls.clone();
                        let sel_const = format!("sel_registerName(\"{}\")", setter_sel);
                        CgExpr {
                            kind: CgExprKind::Call, type_str, line, col,
                            data: CgExprData::Call {
                                name: setter_sel.replace(':', "_"),
                                args: vec![obj_cg, sum_cg],
                                vtable_class,
                                alt_vtable_classes: vec![],
                                is_class_method: false,
                                is_super: false,
                                sel_const_name: Some(sel_const),
                            }
                        }
                    } else {
                        let op_str = op_to_str(*op, false);
                        CgExpr { kind: CgExprKind::Binary, type_str, line, col, data: CgExprData::Binary { op_str: op_str.into(), left: Box::new(convert_expr(left, &class_infos)), right: Box::new(convert_expr(right, &class_infos)) } }
                    }
                } else {
                    let op_str = op_to_str(*op, false);
                    CgExpr { kind: CgExprKind::Binary, type_str, line, col, data: CgExprData::Binary { op_str: op_str.into(), left: Box::new(convert_expr(left, &class_infos)), right: Box::new(convert_expr(right, &class_infos)) } }
                }
            } else {
                let op_str = op_to_str(*op, false);
                CgExpr { kind: CgExprKind::Binary, type_str, line, col, data: CgExprData::Binary { op_str: op_str.into(), left: Box::new(convert_expr(left, &class_infos)), right: Box::new(convert_expr(right, &class_infos)) } }
            }
        }
        AstExprData::Assign { target, value } => {
            if let AstExprData::PropRef { obj, name, is_arrow, prop, .. } = &target.data {
                // Only prepend `_` for ObjC property access (prop is Some). For plain
                // C struct field access (`struct.field = val`), use the name as-is.
                let field_name = if prop.is_some() { format!("_{}", name) } else { name.clone() };
                let obj_cg = convert_expr(obj, &class_infos);
                let value_cg = convert_expr(value, &class_infos);
                // For non-ObjC struct field access (`raw.c_lflag`, `obj.field`) emit `.`
                // via Member; for pointer-to-struct (`obj->field`) emit `->` via Arrow.
                // The previous code unconditionally emitted `->`, breaking
                // `struct termios raw; raw.c_lflag = ...` (raw is not a pointer).
                let target_cg = if *is_arrow {
                    CgExpr { kind: CgExprKind::Arrow, type_str: None, line, col,
                             data: CgExprData::Arrow { obj: Box::new(obj_cg), field: field_name } }
                } else {
                    CgExpr { kind: CgExprKind::Member, type_str: None, line, col,
                             data: CgExprData::Member { obj: Box::new(obj_cg), field: field_name } }
                };
                CgExpr {
                    kind: CgExprKind::Assign, type_str, line, col,
                    data: CgExprData::Assign {
                        target: Box::new(target_cg),
                        value: Box::new(value_cg),
                    },
                }
            } else {
                CgExpr { kind: CgExprKind::Assign, type_str, line, col, data: CgExprData::Assign { target: Box::new(convert_expr(target, &class_infos)), value: Box::new(convert_expr(value, &class_infos)) } }
            }
        }
        AstExprData::Cast { target_type, expr } => {
            let ct = ast_type_to_c_str(target_type);
            CgExpr { kind: CgExprKind::Cast, type_str, line, col, data: CgExprData::Cast { target_type: ct, expr: Box::new(convert_expr(expr, &class_infos)) } }
        }
        AstExprData::Comma(exprs) => {
            CgExpr { kind: CgExprKind::Comma, type_str, line, col, data: CgExprData::Comma(exprs.iter().map(|e| convert_expr(e, &class_infos)).collect()) }
        }
        AstExprData::Subscript { object, key } => {
            // If the object is a PropRef, use ivar access directly instead of getter call
            if let AstExprData::PropRef { obj, name, is_arrow, prop, .. } = &object.data {
                // Only prepend `_` for ObjC property access (prop is Some).
                let field_name = if prop.is_some() { format!("_{}", name) } else { name.clone() };
                let obj_cg = convert_expr(obj, &class_infos);
                let key_cg = convert_expr(key, &class_infos);
                let mut arr_obj = obj_cg;
                // Use _self for ivar access (method body declares _self as casted struct)
                if let CgExprData::Ident(ref name) = arr_obj.data {
                    if name == "self" {
                        arr_obj.data = CgExprData::Ident("_self".into());
                    }
                }
                // Respect `.`/`->` from source: non-ObjC struct field access emits `.`
                // (Member) rather than `->` (Arrow).
                let field_cg = if *is_arrow {
                    CgExpr { kind: CgExprKind::Arrow, type_str: None, line, col,
                             data: CgExprData::Arrow { obj: Box::new(arr_obj), field: field_name.clone() } }
                } else {
                    CgExpr { kind: CgExprKind::Member, type_str: None, line, col,
                             data: CgExprData::Member { obj: Box::new(arr_obj), field: field_name } }
                };
                CgExpr { kind: CgExprKind::Index, type_str, line, col,
                    data: CgExprData::Index {
                        arr: Box::new(field_cg),
                        index: Box::new(key_cg),
                    },
                }
            } else {
                CgExpr { kind: CgExprKind::Index, type_str, line, col, data: CgExprData::Index { arr: Box::new(convert_expr(object, &class_infos)), index: Box::new(convert_expr(key, &class_infos)) } }
            }
        }
        AstExprData::Ternary { cond, then, else_ } => {
            CgExpr { kind: CgExprKind::Ternary, type_str, line, col, data: CgExprData::Ternary { cond: Box::new(convert_expr(cond, &class_infos)), then: Box::new(convert_expr(then, &class_infos)), else_: Box::new(convert_expr(else_, &class_infos)) } }
        }
        AstExprData::ArrayLit(elements) => {
            CgExpr { kind: CgExprKind::InitList, type_str, line, col, data: CgExprData::InitList(elements.iter().map(|e| convert_expr(e, &class_infos)).collect()) }
        }
AstExprData::Selector(s) => {
            let h = fnv1a_hash(s);
            CgExpr { kind: CgExprKind::Int, type_str: Some("unsigned".into()), line, col, data: CgExprData::Int(h as i64) }
        }
        AstExprData::DictLit { .. } => CgExpr { kind: CgExprKind::Ident, type_str, line, col, data: CgExprData::Ident("NULL".into()) },
        AstExprData::PropRef { obj, name, is_arrow, prop, cls, .. } => {
            // ObjC property access (prop is Some, cls is Some): dispatch via vtable getter.
            // Everything else (non-ObjC struct field access like `raw.c_lflag`, or
            // ivar access through a non-self object) is plain C field access —
            // emit `.` (Member) or `->` (Arrow) per `is_arrow`, NOT a vtable call.
            if prop.is_some() && cls.is_some() {
                let sel = name.replace(':', "_");
                let recv_cg = convert_expr(obj, &class_infos);

                let mut vtable_class = None;
                for (_, info) in class_infos.iter() {
                    if info.method_names.iter().position(|n| n == &sanitize_sel_name(&sel)).is_some() {
                        vtable_class = Some(info.class_name.clone());
                        break;
                    }
                }

                let sel_const = format!("sel_registerName(\"{}\")", name);

                CgExpr {
                    kind: CgExprKind::Call, type_str, line, col,
                    data: CgExprData::Call {
                        name: sel, args: vec![recv_cg],
                        vtable_class, alt_vtable_classes: vec![], is_class_method: false, is_super: false,
                        sel_const_name: Some(sel_const),
                    },
                }
            } else {
                // Plain C struct/union field access — respect `.`/`->` from source.
                let recv_cg = convert_expr(obj, &class_infos);
                if *is_arrow {
                    CgExpr { kind: CgExprKind::Arrow, type_str, line, col,
                             data: CgExprData::Arrow { obj: Box::new(recv_cg), field: name.clone() } }
                } else {
                    CgExpr { kind: CgExprKind::Member, type_str, line, col,
                             data: CgExprData::Member { obj: Box::new(recv_cg), field: name.clone() } }
                }
            }
        }
        AstExprData::Sizeof { type_expr, expr } => {
            if let Some(e) = expr {
                CgExpr { kind: CgExprKind::Unary, type_str, line, col, data: CgExprData::Unary { op_str: "sizeof".into(), operand: Box::new(convert_expr(e, &class_infos)), is_postfix: false } }
            } else {
                let type_str_val = ast_type_to_c_str(&type_expr);
                CgExpr { kind: CgExprKind::Sizeof, type_str, line, col, data: CgExprData::Sizeof(type_str_val) }
            }
        }
        AstExprData::Block { params, return_type, body } => {
            let tid = next_temp_id();
            let func_name = format!("__nupa_block_{}", tid);
            let rt = return_type.as_ref().map(|t| ast_type_to_c_str(t)).unwrap_or_else(|| "void".into());
            let mut cg_params = Vec::new();
            if let Some(ref p) = params {
                let mut bp = Some(&**p);
                while let Some(param) = bp {
                    let pt = param.par_type.as_ref()
                        .map(|t| cst_type_to_c_str(t))
                        .unwrap_or_else(|| "int".into());
                    let pn = param.name.clone().unwrap_or_else(|| "_arg".into());
                    cg_params.push((pt, pn));
                    bp = param.next.as_ref().map(|n| &**n);
                }
            }
            let cg_body = body.as_ref().map(|b| Box::new(convert_stmt(b, &class_infos)));
            CgExpr {
                kind: CgExprKind::BlockLit, type_str, line, col,
                data: CgExprData::BlockLit(BlockLiteralData {
                    return_type: rt,
                    params: cg_params,
                    body: cg_body,
                    func_name,
                }),
            }
        }
    }
}

fn convert_stmt(as_: &AstStmt, class_infos: &std::collections::BTreeMap<String, ClassInfo>) -> CgStmt {
    let line = as_.line; let col = as_.col;
    match &as_.data {
        AstStmtData::Expr(e) => CgStmt { kind: CgStmtKind::Expr, line, col, data: CgStmtData::Expr(convert_expr(e, &class_infos)) },
        AstStmtData::Compound(stmts) => CgStmt { kind: CgStmtKind::Compound, line, col, data: CgStmtData::Compound(stmts.iter().map(|s| convert_stmt(s, &class_infos)).collect()) },
        AstStmtData::If { cond, then, else_ } => CgStmt {
            kind: CgStmtKind::If, line, col,
            data: CgStmtData::If { cond: Box::new(convert_expr(cond, &class_infos)), then: Box::new(convert_stmt(then, &class_infos)), else_: else_.as_ref().map(|e| Box::new(convert_stmt(e, &class_infos))) },
        },
        AstStmtData::While { cond, body } => CgStmt {
            kind: CgStmtKind::While, line, col,
            data: CgStmtData::While { cond: Box::new(convert_expr(cond, &class_infos)), body: Box::new(convert_stmt(body, &class_infos)) },
        },
        AstStmtData::Do { body, cond } => CgStmt {
            kind: CgStmtKind::Do, line, col,
            data: CgStmtData::Do { body: Box::new(convert_stmt(body, &class_infos)), cond: Box::new(convert_expr(cond, &class_infos)) },
        },
        AstStmtData::For { init, cond, incr, body } => CgStmt {
            kind: CgStmtKind::For, line, col,
            data: CgStmtData::For {
                init: init.as_ref().map(|i| Box::new(convert_stmt(i, &class_infos))),
                cond: cond.as_ref().map(|c| Box::new(convert_expr(c, &class_infos))),
                incr: incr.as_ref().map(|i| Box::new(convert_expr(i, &class_infos))),
                body: Box::new(convert_stmt(body, &class_infos)),
            },
        },
        AstStmtData::Return(value) => match as_.kind {
            AstStmtKind::Break => CgStmt { kind: CgStmtKind::Break, line, col, data: CgStmtData::Break },
            AstStmtKind::Continue => CgStmt { kind: CgStmtKind::Continue, line, col, data: CgStmtData::Continue },
            _ => CgStmt { kind: CgStmtKind::Return, line, col, data: CgStmtData::Return(value.as_ref().map(|v| Box::new(convert_expr(v, &class_infos)))) },
        }
        AstStmtData::Goto(label) => CgStmt { kind: CgStmtKind::Goto, line, col, data: CgStmtData::Goto(label.clone()) },
        AstStmtData::Label(name) => CgStmt { kind: CgStmtKind::Label, line, col, data: CgStmtData::Label(name.clone()) },
        AstStmtData::Switch { expr, body } => CgStmt { kind: CgStmtKind::Switch, line, col, data: CgStmtData::Switch { expr: Box::new(convert_expr(expr, &class_infos)), body: Box::new(convert_stmt(body, &class_infos)) } },
        AstStmtData::Case { value, body } => CgStmt { kind: CgStmtKind::Case, line, col, data: CgStmtData::Case { value: Box::new(convert_expr(value, &class_infos)), body: Box::new(convert_stmt(body, &class_infos)) } },
        AstStmtData::Default(body) => CgStmt { kind: CgStmtKind::Default, line, col, data: CgStmtData::Default(Box::new(convert_stmt(body, &class_infos))) },
        AstStmtData::Decl(d) => {
            let decls = convert_decl(d, &class_infos);
            if let Some(decl_cg) = decls.into_iter().next() {
                match decl_cg.data {
                    CgDeclData::Variable { var_type, init, is_static, is_weak, next, .. } => {
                        let (decl_type, array_suffix) = if let Some(pos) = var_type.find('[') {
                            (var_type[..pos].trim().to_string(), Some(var_type[pos..].to_string()))
                        } else {
                            (var_type, None)
                        };
                        CgStmt {
                            kind: CgStmtKind::Decl, line, col,
                            data: CgStmtData::Decl { decl_type, name: decl_cg.name, init, array_suffix, is_static, is_weak, next },
                        }
                    }
                    _ => CgStmt { kind: CgStmtKind::Empty, line, col, data: CgStmtData::Return(None) },
                }
            } else {
                CgStmt { kind: CgStmtKind::Empty, line, col, data: CgStmtData::Return(None) }
            }
        }
        AstStmtData::Autoreleasepool(body) => {
            // Emit @autoreleasepool body as a compound statement (runtime handles push/pop)
            let cg_body = convert_stmt(body, &class_infos);
            if let CgStmtData::Compound(ref stmts) = cg_body.data {
                CgStmt { kind: CgStmtKind::Compound, line, col, data: CgStmtData::Compound(stmts.clone()) }
            } else {
                CgStmt { kind: CgStmtKind::Compound, line, col, data: CgStmtData::Compound(vec![cg_body]) }
            }
        }
        AstStmtData::ForIn { var, collection, body } => {
            // Convert for-in to a simple for loop over the collection
            let var_cg = convert_expr(var, &class_infos);
            let col_cg = convert_expr(collection, &class_infos);
            let cg_body = convert_stmt(body, &class_infos);
            // For now, emit as compound with a comment
            CgStmt { kind: CgStmtKind::Compound, line, col, data: CgStmtData::Compound(Vec::new()) }
        }
        AstStmtData::Synchronized { .. } | AstStmtData::Try { .. } | AstStmtData::Catch { .. } | AstStmtData::Finally(_) => {
            CgStmt { kind: CgStmtKind::Compound, line, col, data: CgStmtData::Compound(Vec::new()) }
        }
        _ => CgStmt { kind: CgStmtKind::Expr, line, col, data: CgStmtData::Expr(CgExpr { kind: CgExprKind::Call, type_str: None, line, col, data: CgExprData::Call { name: "/* stub */".into(), args: Vec::new(), vtable_class: None, alt_vtable_classes: vec![], is_class_method: false, is_super: false, sel_const_name: None } }) },
    }
}

fn convert_decl(ad: &AstDecl, class_infos: &std::collections::BTreeMap<String, ClassInfo>) -> Vec<CgDecl> {
    let name = ad.name.clone().unwrap_or_default();
    let mut result = vec![];
    match ad.kind {
        AstDeclKind::Function => {
            let (return_type, func_params, body) = match &ad.data {
                AstDeclData::Function { return_type, params, body, .. } => {
                    let rt = return_type.as_ref().map(|t| ast_type_to_c_str(t)).unwrap_or_else(|| "int".into());
                    let mut cg_params = Vec::new();
                    let mut p = params.as_ref().map(|b| &**b);
                    while let Some(param) = p {
                        let pt = param.par_type.as_ref()
                            .map(|t| cst_type_to_c_str(t))
                            .unwrap_or_else(|| "int".into());
                        let pn = param.name.clone().unwrap_or_default();
                        cg_params.push((pt, pn));
                        p = param.next.as_ref().map(|n| &**n);
                    }
                    let mut cg_body = body.as_ref().map(|b| Box::new(convert_stmt(b, &class_infos)));
                    if name == "main" && !class_infos.is_empty() {
                        let mut stmts = Vec::new();
                        stmts.push(CgStmt {
                            kind: CgStmtKind::Expr, line: 0, col: 0,
                            data: CgStmtData::Expr(CgExpr {
                                kind: CgExprKind::Call, type_str: None, line: 0, col: 0,
                                    data: CgExprData::Call {
                                        name: "nupa_meta_init".into(),
                                        args: Vec::new(),
                                        vtable_class: None,
                                        alt_vtable_classes: vec![],
                                        is_class_method: false,
                                        is_super: false,
                                        sel_const_name: None,
                                    },
                            }),
                        });
                        if let Some(ref mut b) = cg_body {
                            if let CgStmtData::Compound(ref mut inner) = b.data {
                                stmts.extend(inner.clone());
                            } else {
                                stmts.push(b.as_ref().clone());
                            }
                        }
                        cg_body = Some(Box::new(CgStmt {
                            kind: CgStmtKind::Compound, line: 0, col: 0,
                            data: CgStmtData::Compound(stmts),
                        }));
                    }
                    (rt, cg_params, cg_body)
                }
                _ => ("int".to_string(), Vec::new(), None),
            };
            result.push(CgDecl {
                kind: CgDeclKind::Function, name,
                data: CgDeclData::Function {
                    return_type, params: func_params,
                    is_variadic: false, is_objc_class: false, body,
                },
            });
        }
        AstDeclKind::Variable => {
            let (var_type, init, is_static, is_extern, is_const, is_block_qual, is_weak) = match &ad.data {
                AstDeclData::Variable { var_type, init, is_static, is_extern, is_const, is_block_qual, is_weak, .. } => (
                    var_type.as_ref().map(|t| ast_type_to_c_str(t)).unwrap_or_else(|| "int".into()),
                    init.as_ref().map(|i| Box::new(convert_expr(i, &class_infos))),
                    *is_static,
                    *is_extern,
                    *is_const,
                    *is_block_qual,
                    *is_weak,
                ),
                _ => ("int".into(), None, false, false, false, false, false),
            };
            let var_type = if (is_block_qual || is_weak) && !var_type.contains("__block") {
                format!("__block {}", var_type)
            } else {
                var_type
            };
            // Follow next chain (comma-separated declarators)
            let mut next_decls = Vec::new();
            let mut n = match &ad.data { AstDeclData::Variable { ref next, .. } => next.as_ref().map(|b| &**b), _ => None };
            while let Some(next_ad) = n {
                let next_name = next_ad.name.clone().unwrap_or_default();
                let next_init = match &next_ad.data {
                    AstDeclData::Variable { init, .. } => init.as_ref().map(|i| Box::new(convert_expr(i, &class_infos))),
                    _ => None,
                };
                next_decls.push((next_name, next_init));
                n = match &next_ad.data { AstDeclData::Variable { ref next, .. } => next.as_ref().map(|b| &**b), _ => None };
            }
            result.push(CgDecl {
                kind: CgDeclKind::Variable, name,
                data: CgDeclData::Variable { var_type, init, is_static, is_const, is_weak, next: next_decls },
            });
        }
        AstDeclKind::Typedef => {
            let (alias_type_str, struct_fields, has_block_name) = match &ad.data {
                AstDeclData::Typedef { aliased_type, struct_fields } => {
                    let mut alias_type_str = "int".to_string();
                    let mut has_block_name = false;
                    if let Some(ref at) = aliased_type {
                        if at.block_name.is_some() {
                            // For block typedefs, emit with short name first (visible inside namespace)
                            alias_type_str = ast_type_to_c_str(at);
                            has_block_name = true;
                        } else {
                            alias_type_str = ast_type_to_c_str(at);
                        }
                    }
                    let fields = struct_fields.iter().map(|f| {
                        let fname = f.name.clone().unwrap_or_default();
                        let ftype = match &f.data {
                            AstDeclData::Variable { var_type, .. } => var_type.as_ref().map(|t| ast_type_to_c_str(t)).unwrap_or_else(|| "int".into()),
                            _ => "int".into(),
                        };
                        (ftype, fname)
                    }).collect();
                    (alias_type_str, fields, has_block_name)
                }
                _ => ("int".into(), Vec::new(), false),
            };
            let flat_alias = name_flat(&name);
            let alias = if has_block_name { String::new() } else { flat_alias.clone() };
            result.push(CgDecl {
                kind: CgDeclKind::Typedef, name: ad.name.clone().unwrap_or_default(),
                data: CgDeclData::Typedef { alias, type_str: alias_type_str.clone(), struct_fields },
            });
            // For block typedefs, also emit a typedef with the flat (namespace-mangled) name
            // so fully-qualified references from outside the namespace resolve.
            if has_block_name {
                let flat_type_str = match &ad.data {
                    AstDeclData::Typedef { aliased_type, .. } => {
                        if let Some(ref at) = aliased_type {
                            let mut at_mod = at.as_ref().clone();
                            at_mod.block_name = Some(flat_alias);
                            ast_type_to_c_str(&at_mod)
                        } else {
                            alias_type_str.clone()
                        }
                    }
                    _ => alias_type_str.clone(),
                };
                result.push(CgDecl {
                    kind: CgDeclKind::Typedef, name: ad.name.clone().unwrap_or_default(),
                    data: CgDeclData::Typedef { alias: String::new(), type_str: flat_type_str, struct_fields: vec![] },
                });
            }
        }
        AstDeclKind::Struct => {
            let fields = match &ad.data {
                AstDeclData::Aggregate { fields } => fields,
                _ => &Vec::new(),
            };
            if fields.is_empty() {
                result.push(CgDecl { kind: CgDeclKind::Variable, name, data: CgDeclData::Variable { var_type: "void".into(), init: None, is_static: false, is_const: false, is_weak: false, next: vec![] } });
            } else {
                result.push(CgDecl { kind: CgDeclKind::Struct, name, data: CgDeclData::Struct { fields: Vec::new() } });
            }
        }
        AstDeclKind::Enum => {
            let members: Vec<(String, String)> = match &ad.data {
                AstDeclData::Enum { members, values } => {
                    members.iter().zip(values.iter()).map(|(m, v)| {
                        let val = match &v.data { AstExprData::Int(i) => format!("{}", i), _ => m.clone() };
                        (m.clone(), val)
                    }).collect()
                }
                _ => Vec::new(),
            };
            result.push(CgDecl { kind: CgDeclKind::Enum, name, data: CgDeclData::Enum { members } });
        }
        AstDeclKind::Class | AstDeclKind::Protocol => {
            result.push(CgDecl { kind: CgDeclKind::Variable, name, data: CgDeclData::Variable { var_type: "void".into(), init: None, is_static: false, is_const: false, is_weak: false, next: vec![] } });
        }
        AstDeclKind::Ivar | AstDeclKind::Method | AstDeclKind::Property | AstDeclKind::Union | AstDeclKind::Namespace => {
            result.push(CgDecl { kind: CgDeclKind::Variable, name, data: CgDeclData::Variable { var_type: "int".into(), init: None, is_static: false, is_const: false, is_weak: false, next: vec![] } });
        }
    }
    result
}

// ─── AST → CgUnit ────────────────────────────────────────────────────────────

pub fn method_c_name(msym_name: &str, class_name: &str) -> String {
    let flat_cname = name_flat(class_name);
    let flat_msym = msym_name.replace(':', "_");
    format!("{}_{}", flat_cname, flat_msym)
}

fn split_array_type(t: &str) -> (&str, &str) {
    if let Some(pos) = t.find('[') {
        (&t[..pos].trim(), &t[pos..])
    } else {
        (t, "")
    }
}

/// Format a parameter declaration, handling block types specially.
/// Block types like `void (^)(char)` with name `resultBlock` must be emitted
/// as `void (^resultBlock)(char)`, not `void (^)(char) resultBlock`.
fn format_param_decl(pt: &str, pn: &str) -> String {
    // Check if this is a block type: contains `(^)`
    if let Some(block_pos) = pt.find("(^)") {
        let before = &pt[..block_pos];
        let after = &pt[block_pos + 3..];
        format!("{}(^{}){}", before, pn, after)
    } else if let Some(block_pos) = pt.find("(^") {
        let before = &pt[..block_pos + 2];
        let after = &pt[block_pos + 2..];
        format!("{}{}{}", before, pn, after)
    } else {
        let (base, arr_suffix) = split_array_type(pt);
        format!("{} {}{}", base, pn, arr_suffix)
    }
}

pub fn ast_to_cg_unit(ast: &AstUnit) -> CgUnit {
    let mut selectors = Vec::new();
    let mut classes: Vec<CgClassMeta> = Vec::new();
    let mut decls: Vec<CgDecl> = Vec::new();

    fn add_sel(selectors: &mut Vec<String>, sel: &str) {
        let sn = sel_const_name(sel);
        if !selectors.iter().any(|s| sel_const_name(s) == sn) {
            selectors.push(sel.to_string());
        }
    }

    // First pass: collect interface metadata (method signatures)
    use std::collections::BTreeMap;
    let mut class_infos: BTreeMap<String, ClassInfo> = BTreeMap::new();

    // Pre‑pass: register all class entries + method names so that
    // vtable dispatch resolution in method‑body conversion can find
    // protocol methods implemented in classes defined later in source
    // order (e.g. NPCollectionInspector implements collectionDidReachCapacity:
    // but NPDataContainer's method body that calls it is converted first).
    for d in &ast.decls {
        if d.kind != AstDeclKind::Class { continue; }
        let cls_name = d.name.clone().unwrap_or_default();
        let flat = name_flat(&cls_name);
        let super_name = match &d.data {
            AstDeclData::Class { super_name, .. } => super_name.clone(),
            _ => None,
        };
        class_infos.entry(flat.clone()).or_insert(ClassInfo {
            class_name: cls_name,
            flat: flat.clone(),
            super_name,
            method_names: Vec::new(),
            is_class_methods: Vec::new(),
            method_bodies: Vec::new(),
            method_return_types: Vec::new(),
            method_params_list: Vec::new(),
            method_owners: Vec::new(),
            ivar_types: Vec::new(),
            ivar_names: Vec::new(),
        });
        if let AstDeclData::Class { methods: ref class_methods, .. } = &d.data {
            for m in class_methods {
                if let Some(ref mname) = m.name {
                    let sanitized = sanitize_sel_name(mname);
                    if let Some(info) = class_infos.get_mut(&flat) {
                        if !info.method_names.contains(&sanitized) {
                            let is_class = match &m.data {
                                AstDeclData::Method { is_class_method, .. } => *is_class_method,
                                _ => false,
                            };
                            info.method_names.push(sanitized);
                            info.is_class_methods.push(is_class);
                            info.method_bodies.push(None);
                            info.method_return_types.push(String::new());
                            info.method_params_list.push(Vec::new());
                            info.method_owners.push(flat.clone());
                        }
                    }
                }
            }
        }
    }

    for d in &ast.decls {
        if d.kind != AstDeclKind::Class { continue; }
        let cls_name = d.name.clone().unwrap_or_default();
        let flat = name_flat(&cls_name);
        let cls_name_for_synth = cls_name.clone();
        let super_name = match &d.data {
            AstDeclData::Class { super_name, .. } => super_name.clone(),
            _ => None,
        };
        class_infos.entry(flat.clone()).or_insert(ClassInfo {
            class_name: cls_name,
            flat: flat.clone(),
            super_name,
            method_names: Vec::new(),
            is_class_methods: Vec::new(),
            method_bodies: Vec::new(),
            method_return_types: Vec::new(),
            method_params_list: Vec::new(),
            method_owners: Vec::new(),
            ivar_types: Vec::new(),
            ivar_names: Vec::new(),
        });

        if let AstDeclData::Class { methods: ref class_methods, ivars: ref class_ivars, properties: ref class_properties, .. } = &d.data {
            let mut ivar_types = Vec::new();
            let mut ivar_names = Vec::new();

            for iv in class_ivars {
                if let AstDeclData::Ivar { ivar_type, .. } = &iv.data {
                    let it = ivar_type.as_ref().map(|t| ast_type_to_c_str(t)).unwrap_or_else(|| "int".into());
                    let in_ = iv.name.clone().unwrap_or_default();
                    ivar_types.push(it);
                    ivar_names.push(in_);
                }
            }

            // Store ivar data in class_infos. MERGE with any existing ivars
            // (e.g. main @interface declares `_sequencerId`, then a category
            // `@property` synthesizes `_neuralSyncRate`). The previous
            // `is_empty()` guard dropped the main-class ivars when the category
            // synthesized its own, producing a struct missing the main ivars.
            if let Some(info) = class_infos.get_mut(&flat) {
                for (idx, n) in ivar_names.iter().enumerate() {
                    if !info.ivar_names.contains(n) {
                        info.ivar_names.push(n.clone());
                        info.ivar_types.push(ivar_types[idx].clone());
                    }
                }
            }

            for m in class_methods {
                if let Some(ref mname) = m.name {
                    let sel = mname.clone();
                    add_sel(&mut selectors, &sel);

                    let ret_type = match &m.data {
                        AstDeclData::Method { return_type, .. } =>
                            return_type.as_ref().map(|t| ast_type_to_c_str(t)).unwrap_or_else(|| "NPObject *".into()),
                        _ => "NPObject *".into(),
                    };
                    let is_class = match &m.data {
                        AstDeclData::Method { is_class_method, .. } => *is_class_method,
                        _ => false,
                    };

                    let fn_name = format!("{}_{}", flat, sanitize_sel_name(&sel));

                    let mut fn_params = Vec::new();
                    let self_type = if is_class { "NPClass *" } else { "NPObject *" };
                    fn_params.push((self_type.to_string(), "self".to_string()));
                    fn_params.push(("SEL".to_string(), "_cmd".to_string()));

                    if let AstDeclData::Method { params: ref method_params, .. } = m.data {
                        let mut p = method_params.as_ref().map(|b| &**b);
                        while let Some(param) = p {
                            if let Some(ref pt) = param.par_type {
                                fn_params.push((cst_type_to_c_str(pt), param.name.clone().unwrap_or_else(|| "_arg".into())));
                            }
                            p = param.next.as_ref().map(|n| &**n);
                        }
                    }

                    let body = match &m.data {
                        AstDeclData::Method { body, .. } => {
                            body.as_ref().map(|b| {
                                let cg_body = convert_stmt(b, &class_infos);
                                if !is_class {
                                    let mut stmts = Vec::new();
                                    stmts.push(CgStmt {
                                        kind: CgStmtKind::Decl, line: 0, col: 0,
                                        data: CgStmtData::Decl {
                                            decl_type: format!("struct {} *", flat),
                                            name: "_self".into(),
                                            next: vec![],
                                            init: Some(Box::new(CgExpr {
                                                kind: CgExprKind::Cast, type_str: None, line: 0, col: 0,
                                                data: CgExprData::Cast {
                                                    target_type: format!("struct {} *", flat),
                                                    expr: Box::new(CgExpr { kind: CgExprKind::Ident, type_str: None, line: 0, col: 0, data: CgExprData::Ident("self".into()) }),
                                                },
                                            })),
                                            array_suffix: None,
                                            is_static: false,
                                            is_weak: false,
                                        },
                                    });
                                    stmts.push(cg_body);
                                    CgStmt { kind: CgStmtKind::Compound, line: 0, col: 0, data: CgStmtData::Compound(stmts) }
                                } else {
                                    cg_body
                                }
                            })
                        }
                        _ => None,
                    };

                    let info = class_infos.get_mut(&flat).unwrap();
                    if let Some(idx) = info.method_names.iter().position(|n| *n == sanitize_sel_name(&sel)) {

                        // Method name registered (possibly by pre‑pass or an earlier @interface).
                        if let Some(body_val) = body {
                            // @implementation provides a body
                            if let Some(existing) = decls.iter_mut().find(|d| d.name == fn_name) {
                                match existing.data {
                                    CgDeclData::Function { ref mut body, ref mut params, .. } => {
                                        *body = Some(Box::new(body_val.clone()));
                                        // Also update method_bodies for generic instantiation cloning
                                        if idx < info.method_bodies.len() {
                                            info.method_bodies[idx] = Some(Box::new(body_val.clone()));
                                        }
                                        // Update return type & params in class_infos (pre‑pass may
                                        // have left empty placeholders that vtable struct emission reads).
                                        if idx < info.method_return_types.len() {
                                            info.method_return_types[idx] = ret_type.clone();
                                        }
                                        if idx < info.method_params_list.len() {
                                            info.method_params_list[idx] = fn_params.clone();
                                        }
                                        // Update parameter names from the @implementation method
                                        for i in 2..params.len().min(fn_params.len()) {
                                            if fn_params[i].1 != "value" && fn_params[i].1 != "_arg" && !fn_params[i].1.is_empty() {
                                                params[i].1 = fn_params[i].1.clone();
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            } else {
                                // Method was registered in pre‑pass but no CgDecl exists yet
                                // (e.g. @interface came after pre‑pass but before @implementation,
                                //  or the @interface had no body and the @implementation is first).
                                // Create it now with the body.
                                if idx < info.method_bodies.len() {
                                    info.method_bodies[idx] = Some(Box::new(body_val.clone()));
                                }
                                if idx < info.method_return_types.len() {
                                    info.method_return_types[idx] = ret_type.clone();
                                }
                                if idx < info.method_params_list.len() {
                                    info.method_params_list[idx] = fn_params.clone();
                                }
                                decls.push(CgDecl {
                                    kind: CgDeclKind::Function, name: fn_name,
                                    data: CgDeclData::Function {
                                        return_type: ret_type, params: fn_params,
                                        is_variadic: false, is_objc_class: true,
                                        body: Some(Box::new(body_val.clone())),
                                    },
                                });
                            }
                        } else if !decls.iter().any(|d| d.name == fn_name) {
                            // @interface method with no body → create forward declaration
                            // so that the function exists even without @implementation.
                            if idx < info.method_return_types.len() {
                                info.method_return_types[idx] = ret_type.clone();
                            }
                            if idx < info.method_params_list.len() {
                                info.method_params_list[idx] = fn_params.clone();
                            }
                            decls.push(CgDecl {
                                kind: CgDeclKind::Function, name: fn_name,
                                data: CgDeclData::Function {
                                    return_type: ret_type, params: fn_params,
                                    is_variadic: false, is_objc_class: true,
                                    body: None,
                                },
                            });
                        }
                    } else {
                        info.method_names.push(sanitize_sel_name(&sel));
                        info.is_class_methods.push(is_class);
                        info.method_bodies.push(body.as_ref().map(|b| Box::new(b.clone())));
                        info.method_return_types.push(ret_type.clone());
                        info.method_params_list.push(fn_params.clone());
                        info.method_owners.push(flat.clone());

                        decls.push(CgDecl {
                            kind: CgDeclKind::Function, name: fn_name,
                            data: CgDeclData::Function {
                                return_type: ret_type, params: fn_params,
                                is_variadic: false, is_objc_class: true,
                                body: body.map(Box::new),
                            },
                        });
                    }
                }
            }

            for prop in class_properties {
                if let AstDeclData::Property { prop_type, is_readonly, is_dynamic, is_weak, .. } = &prop.data {
                    let prop_name = prop.name.clone().unwrap_or_default();
                    let ivar_name = format!("_{}", prop_name);
                    let getter_name = prop_name.clone();
                    let setter_name = format!("set{}{}:", 
                        getter_name[0..1].to_uppercase(),
                        &getter_name[1..]);

                    let it = prop_type.as_ref().map(|t| ast_type_to_c_str(t)).unwrap_or_else(|| "int".into());
                    if !ivar_names.contains(&ivar_name) {
                        ivar_types.push(it.clone());
                        ivar_names.push(ivar_name.clone());
                    }

                    // Skip getter/setter generation for array properties (C cannot return arrays from functions)
                    let is_array_prop = prop_type.as_ref().map_or(false, |t| t.is_array);
                    if is_array_prop { continue; }

                    // @dynamic: getter/setter provided by @implementation, skip synthesis
                    if *is_dynamic { continue; }

                    let getter_sel = getter_name.replace(':', "_");
                    let getter_fn_name = format!("{}_{}", flat, getter_sel);
                    add_sel(&mut selectors, &getter_sel);
                    
                    let getter_return_expr = AstExpr {
                        kind: AstExprKind::IvarRef,
                        expr_type: prop_type.clone(),
                        line: 0, col: 0,
                        data: AstExprData::IvarRef {
                            ivar: Some(ivar_name.clone()),
                            cls: Some(cls_name_for_synth.clone()),
                            obj: Box::new(AstExpr {
                                kind: AstExprKind::Cast,
                                expr_type: None,
                                line: 0, col: 0,
                                data: AstExprData::Cast {
                                    target_type: AstType {
                                        prim: nupa_cst::TypePrim::Named,
                                        is_pointer: true,
                                        is_struct: true,
                                        name: Some(flat.clone()),
                                        ..AstType::new(nupa_cst::TypePrim::Named)
                                    },
                                    expr: Box::new(AstExpr {
                                        kind: AstExprKind::Self_,
                                        expr_type: None,
                                        line: 0, col: 0,
                                        data: AstExprData::VarRef { sym: None, name: "self".into() },
                                    }),
                                },
                            }),
                        },
                    };
                    let getter_body = AstStmt {
                        kind: AstStmtKind::Compound,
                        line: 0, col: 0,
                        data: AstStmtData::Compound(vec![
                            AstStmt {
                                kind: AstStmtKind::Return,
                                line: 0, col: 0,
                                data: AstStmtData::Return(Some(Box::new(getter_return_expr))),
                            }
                        ]),
                    };

                    let getter_params = vec![("NPObject *".to_string(), "self".to_string()), ("SEL".to_string(), "_cmd".to_string())];
                    let getter_params_clone = getter_params.clone();
                    // Only create CgDecl if it doesn't already exist (e.g. pre‑pass or @implementation registered it).
                    if decls.iter().any(|d| d.name == getter_fn_name) {
                        // Update return type and params in class_infos (pre‑pass may have placeholders).
                        let info = class_infos.get_mut(&flat).unwrap();
                        if let Some(gidx) = info.method_names.iter().position(|n| *n == getter_sel) {
                            if gidx < info.method_return_types.len() {
                                info.method_return_types[gidx] = it.clone();
                            }
                            if gidx < info.method_params_list.len() {
                                info.method_params_list[gidx] = getter_params_clone;
                            }
                        }
                    } else {
                        decls.push(CgDecl {
                            kind: CgDeclKind::Function, name: getter_fn_name.clone(),
                            data: CgDeclData::Function {
                                return_type: it.clone(), params: getter_params,
                                is_variadic: false, is_objc_class: true,
                                body: Some(Box::new(convert_stmt(&getter_body, &class_infos))),
                            },
                        });
                        let info = class_infos.get_mut(&flat).unwrap();
                        if !info.method_names.contains(&getter_sel) {
                            info.method_names.push(getter_sel);
                            info.is_class_methods.push(false);
                            info.method_return_types.push(it.clone());
                            info.method_params_list.push(getter_params_clone);
                            info.method_owners.push(flat.clone());
                        }
                    }

                    if !*is_readonly {
                        let setter_sel = setter_name.replace(':', "_");
                        let setter_fn_name = format!("{}_{}", flat, setter_sel);
                        add_sel(&mut selectors, &setter_sel);

                        let setter_assign_expr = AstExpr {
                            kind: AstExprKind::Assign,
                            expr_type: None,
                            line: 0, col: 0,
                            data: AstExprData::Assign {
                                target: Box::new(AstExpr {
                                    kind: AstExprKind::IvarRef,
                                    expr_type: None,
                                    line: 0, col: 0,
                                data: AstExprData::IvarRef {
                                    ivar: Some(ivar_name.clone()),
                                    cls: Some(cls_name_for_synth.clone()),
                                    obj: Box::new(AstExpr {
                                        kind: AstExprKind::Cast,
                                        expr_type: None,
                                        line: 0, col: 0,
                                        data: AstExprData::Cast {
                                            target_type: AstType {
                                                prim: nupa_cst::TypePrim::Named,
                                                is_pointer: true,
                                                is_struct: true,
                                                name: Some(flat.clone()),
                                                ..AstType::new(nupa_cst::TypePrim::Named)
                                            },
                                            expr: Box::new(AstExpr {
                                                kind: AstExprKind::Self_,
                                                expr_type: None,
                                                line: 0, col: 0,
                                                data: AstExprData::VarRef { sym: None, name: "self".into() },
                                            }),
                                        },
                                    }),
                                },
                                }),
                                value: Box::new(AstExpr {
                                    kind: AstExprKind::VarRef,
                                    expr_type: None,
                                    line: 0, col: 0,
                                    data: AstExprData::VarRef { sym: None, name: "value".into() },
                                }),
                            },
                        };
                        let setter_body = AstStmt {
                            kind: AstStmtKind::Compound,
                            line: 0, col: 0,
                            data: AstStmtData::Compound(vec![
                                AstStmt {
                                    kind: AstStmtKind::Expr,
                                    line: 0, col: 0,
                                    data: AstStmtData::Expr(setter_assign_expr),
                                }
                            ]),
                        };

                        let mut setter_params = vec![("NPObject *".to_string(), "self".to_string()), ("SEL".to_string(), "_cmd".to_string())];
                        // Use the original parameter name from the @implementation method if available
                        let existing_param_name = decls.iter()
                            .find(|d| d.name == setter_fn_name)
                            .and_then(|d| {
                                if let CgDeclData::Function { ref params, .. } = d.data {
                                    params.get(2).map(|(_, n)| n.clone())
                                } else { None }
                            })
                            .unwrap_or_else(|| "value".to_string());
                        setter_params.push((it, existing_param_name.clone()));
                        let setter_params_clone = setter_params.clone();
                        // If the method already exists in decls (from @implementation), skip adding a new declaration
                        // Just update the vtable info
                        if decls.iter().any(|d| d.name == setter_fn_name) {
                            let info = class_infos.get_mut(&flat).unwrap();
                            if !info.method_names.contains(&setter_sel) {
                                info.method_names.push(setter_sel);
                                info.is_class_methods.push(false);
                                info.method_return_types.push("void".to_string());
                                info.method_params_list.push(setter_params_clone);
                                info.method_owners.push(flat.clone());
                            }
                            // Also fix the existing declaration's parameter name if it's still "value"
                            if let Some(existing) = decls.iter_mut().find(|d| d.name == setter_fn_name) {
                                if let CgDeclData::Function { ref mut params, .. } = existing.data {
                                    if params.len() > 2 && existing_param_name != "value" {
                                        params[2].1 = existing_param_name;
                                    }
                                }
                            }
                            continue;
                        }
                        if *is_weak {
                            let ivar_expr = CgExpr {
                                kind: CgExprKind::Arrow, type_str: None, line: 0, col: 0,
                                data: CgExprData::Arrow {
                                    obj: Box::new(CgExpr {
                                        kind: CgExprKind::Cast, type_str: None, line: 0, col: 0,
                                        data: CgExprData::Cast {
                                            target_type: format!("struct {} *", flat),
                                            expr: Box::new(CgExpr {
                                                kind: CgExprKind::Ident, type_str: None, line: 0, col: 0,
                                                data: CgExprData::Ident("self".into()),
                                            }),
                                        },
                                    }),
                                    field: ivar_name.clone(),
                                },
                            };
                            let addr_of_ivar = CgExpr {
                                kind: CgExprKind::Unary, type_str: None, line: 0, col: 0,
                                data: CgExprData::Unary {
                                    op_str: "&".into(), operand: Box::new(ivar_expr.clone()), is_postfix: false,
                                },
                            };
                            let cast_addr = CgExpr {
                                kind: CgExprKind::Cast, type_str: None, line: 0, col: 0,
                                data: CgExprData::Cast {
                                    target_type: "NPObject **".into(), expr: Box::new(addr_of_ivar),
                                },
                            };
                            let value_ident = CgExpr {
                                kind: CgExprKind::Ident, type_str: None, line: 0, col: 0,
                                data: CgExprData::Ident(existing_param_name.clone()),
                            };
                            let cast_value = CgExpr {
                                kind: CgExprKind::Cast, type_str: None, line: 0, col: 0,
                                data: CgExprData::Cast {
                                    target_type: "NPObject *".into(), expr: Box::new(value_ident),
                                },
                            };
                            let assign_expr = CgExpr {
                                kind: CgExprKind::Assign, type_str: None, line: 0, col: 0,
                                data: CgExprData::Assign {
                                    target: Box::new(ivar_expr),
                                    value: Box::new(CgExpr {
                                        kind: CgExprKind::Ident, type_str: None, line: 0, col: 0,
                                        data: CgExprData::Ident(existing_param_name.clone()),
                                    }),
                                },
                            };
                            decls.push(CgDecl {
                                kind: CgDeclKind::Function, name: setter_fn_name,
                                data: CgDeclData::Function {
                                    return_type: "void".to_string(), params: setter_params,
                                    is_variadic: false, is_objc_class: true,
                                    body: Some(Box::new(CgStmt {
                                        kind: CgStmtKind::Compound, line: 0, col: 0,
                                        data: CgStmtData::Compound(vec![
                                            CgStmt { kind: CgStmtKind::Expr, line: 0, col: 0, data: CgStmtData::Expr(CgExpr {
                                                kind: CgExprKind::Call, type_str: None, line: 0, col: 0,
                                                data: CgExprData::Call {
                                                    name: "nupa_weak_unregister".into(),
                                                    args: vec![cast_addr.clone()],
                                                    vtable_class: None, alt_vtable_classes: vec![], is_class_method: false, is_super: false, sel_const_name: None,
                                                },
                                            })},
                                            CgStmt { kind: CgStmtKind::Expr, line: 0, col: 0, data: CgStmtData::Expr(assign_expr.clone()) },
                                            CgStmt { kind: CgStmtKind::Expr, line: 0, col: 0, data: CgStmtData::Expr(CgExpr {
                                                kind: CgExprKind::Call, type_str: None, line: 0, col: 0,
                                                data: CgExprData::Call {
                                                    name: "nupa_weak_register".into(),
                                                    args: vec![cast_addr.clone(), cast_value],
                                                    vtable_class: None, alt_vtable_classes: vec![], is_class_method: false, is_super: false, sel_const_name: None,
                                                },
                                            })},
                                        ]),
                                    })),
                                },
                            });
                        } else {
                            decls.push(CgDecl {
                                kind: CgDeclKind::Function, name: setter_fn_name,
                                data: CgDeclData::Function {
                                    return_type: "void".to_string(), params: setter_params,
                                    is_variadic: false, is_objc_class: true,
                                    body: Some(Box::new(convert_stmt(&setter_body, &class_infos))),
                                },
                            });
                        }
                        let info = class_infos.get_mut(&flat).unwrap();
                        if !info.method_names.contains(&setter_sel) {
                            info.method_names.push(setter_sel);
                            info.is_class_methods.push(false);
                            info.method_return_types.push("void".to_string());
                            info.method_params_list.push(setter_params_clone);
                            info.method_owners.push(flat.clone());
                        }
                    }
                }
            }

            let info = class_infos.get_mut(&flat).unwrap();
            // MERGE synthesized ivars with any existing ones (e.g. main
            // @interface declares `_sequencerId`, then a category @property
            // synthesizes `_neuralSyncRate`). The previous assignment
            // `info.ivar_names = ivar_names` dropped the main-class ivars.
            for (idx, n) in ivar_names.iter().enumerate() {
                if !info.ivar_names.contains(n) {
                    info.ivar_names.push(n.clone());
                    info.ivar_types.push(ivar_types[idx].clone());
                }
            }
        }
    }

    // Flatten nested namespace decls into the top-level decls stream so that
    // typedef aliases (e.g. Block typedef `void (^ActionCompleteBlock)(...)`)
    // declared inside `@namespace { ... }` reach the C output. Without this,
    // codegen treats Namespace as a stub (convert_decl ~line 958) and silently
    // drops the typedef aliases it contains, causing `unknown type name` errors.
    fn flatten_namespace_decls<'a>(ad: &'a AstDecl, out: &mut Vec<&'a AstDecl>) {
        if ad.kind == AstDeclKind::Namespace {
            if let AstDeclData::Namespace(inner) = &ad.data {
                for d in inner { flatten_namespace_decls(d, out); }
            }
            return;
        }
        out.push(ad);
    }
    let mut flat_decls: Vec<&AstDecl> = Vec::new();
    for d in &ast.decls { flatten_namespace_decls(d, &mut flat_decls); }

    for d in flat_decls {
        if d.kind != AstDeclKind::Class {
            // For @implementation methods, update the parameter names of the existing
            // synthetic property getter/setter declarations (e.g. "value" → real parameter name "s")
            if d.kind == AstDeclKind::Method {
                if let AstDeclData::Method { params: method_params, .. } = &d.data {
                    let sel_sanitized = d.name.as_deref()
                        .map(|n| n.replace(':', "_"))
                        .unwrap_or_default();
                    if let Some(existing) = decls.iter_mut().find(|e| e.name.ends_with(&sel_sanitized)) {
                        if let CgDeclData::Function { ref mut params, .. } = existing.data {
                            let mut p = method_params.as_ref().map(|b| &**b);
                            let mut idx = 2; // skip self, _cmd
                            while let Some(param) = p {
                                if idx < params.len() {
                                    let pn = param.name.clone().unwrap_or_default();
                                    if pn != "value" && pn != "_arg" && !pn.is_empty() {
                                        params[idx].1 = pn;
                                    }
                                }
                                idx += 1;
                                p = param.next.as_ref().map(|n| &**n);
                            }
                        }
                        continue; // skip adding a new declaration
                    }
                }
            }
            // Namespace: recurse into the namespace body and push each inner decl.
            if d.kind == AstDeclKind::Namespace {
                if let AstDeclData::Namespace(inner) = &d.data {
                    for inner_d in inner.iter() {
                        for cg in convert_decl(inner_d, &class_infos) {
                            // Skip the stub markers used for ivar/method/property
                            if cg.kind == CgDeclKind::Variable {
                                if let CgDeclData::Variable { ref var_type, .. } = cg.data {
                                    if var_type == "int" || var_type == "void" { continue; }
                                }
                            }
                            decls.push(cg);
                        }
                    }
                }
                continue;
            }
            for cg in convert_decl(d, &class_infos) {
                // Skip empty struct forward declarations (e.g. `struct NPClass;`)
                if cg.kind == CgDeclKind::Variable && cg.name == d.name.clone().unwrap_or_default() {
                    if let CgDeclData::Variable { ref var_type, .. } = cg.data {
                        if var_type == "void" { continue; }
                    }
                }
                decls.push(cg);
            }
        }
    }

    // Extract impl_vars from class declarations (e.g. static globals in categories)
    for d in &ast.decls {
        if d.kind == AstDeclKind::Class {
            if let AstDeclData::Class { impl_vars, .. } = &d.data {
                for v in impl_vars {
                    for cg in convert_decl(v, &class_infos) {
                        decls.push(cg);
                    }
                }
            }
        }
    }

    // ─── Generic instantiation collection ─────────────────────────────────────
    // Scan all decls/exprs for generic class instantiations like
    // `DataPack<QuantumToken*>` (type refs with non-empty type_args) and
    // `[[DataPack<QuantumToken*> alloc] init]` (MsgSend receiver rendered as
    // `Name<T*>` string). For each unique instantiation, clone the generic
    // class's ClassInfo (substituting T → concrete type in ivar/method
    // signatures) under the mangled flat name so codegen emits a standalone
    // struct/vtable/class metadata per instantiation. Without this, references
    // to `nupa_DataPack_QuantumToken_ptr_class` are undeclared.
    let mut generic_instantiations: Vec<(String, Vec<AstType>)> = Vec::new();
    fn collect_instantiations_expr(e: &AstExpr, out: &mut Vec<(String, Vec<AstType>)>) {
        match &e.data {
            AstExprData::MsgSend { receiver, .. } => {
                // Receiver may be a VarRef holding `Name<T*>` (rendered type string)
                if let AstExprData::VarRef { ref name, .. } = receiver.data {
                    if let Some((base, args)) = parse_generic_type_string(name) {
                        out.push((base, args));
                    }
                }
                collect_instantiations_expr(receiver, out);
            }
            _ => {}
        }
    }
    fn parse_generic_type_string(s: &str) -> Option<(String, Vec<AstType>)> {
        // Parse `Name<T1, T2*>` rendered strings into (base, type_args).
        // Returns None if no `<...>` present.
        let lt = s.find('<')?;
        let base = s[..lt].to_string();
        // Find matching `>` (depth-aware for nested generics)
        let mut depth = 0;
        let mut end = None;
        for (i, ch) in s[lt..].char_indices() {
            match ch {
                '<' => depth += 1,
                '>' => {
                    depth -= 1;
                    if depth == 0 { end = Some(lt + i); break; }
                }
                _ => {}
            }
        }
        let end = end?;
        let args_str = &s[lt+1..end];
        let mut args = Vec::new();
        for a in split_top_commas(args_str) {
            args.push(render_type_str_to_ast(&a));
        }
        Some((base, args))
    }
    fn split_top_commas(s: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut depth = 0;
        let mut cur = String::new();
        for ch in s.chars() {
            match ch {
                '<' => { depth += 1; cur.push(ch); }
                '>' => { depth -= 1; cur.push(ch); }
                ',' if depth == 0 => { out.push(cur.trim().to_string()); cur.clear(); }
                _ => cur.push(ch),
            }
        }
        if !cur.trim().is_empty() { out.push(cur.trim().to_string()); }
        out
    }
    fn render_type_str_to_ast(s: &str) -> AstType {
        // Render a type argument string like `QuantumToken*` into an AstType
        // so the substitution logic can mangle it. Keep it minimal: Named base
        // + pointer flag; nested generics handled by recursion when present.
        let s = s.trim();
        let (base, is_ptr) = if let Some(stripped) = s.strip_suffix('*') {
            (stripped.trim().to_string(), true)
        } else {
            (s.to_string(), false)
        };
        // Nested generic args?
        if let Some(lt) = base.find('<') {
            if base.ends_with('>') {
                let inner = &base[lt+1..base.len()-1];
                let inner_args: Vec<AstType> = split_top_commas(inner).iter().map(|s| render_type_str_to_ast(s)).collect();
                let mut t = AstType::new(TypePrim::Named);
                t.name = Some(base[..lt].to_string());
                t.type_args = inner_args;
                t.is_pointer = is_ptr;
                return t;
            }
        }
        let mut t = AstType::new(TypePrim::Named);
        t.name = Some(base);
        t.is_pointer = is_ptr;
        t
    }
    // Walk all decls and statements/expressions to collect instantiations.
    for d in &ast.decls {
        if d.kind == AstDeclKind::Class {
            if let AstDeclData::Class { methods, ivars, .. } = &d.data {
                for m in methods {
                    if let AstDeclData::Method { body, .. } = &m.data {
                        if let Some(b) = body { walk_stmt_for_inst(b, &mut generic_instantiations, &collect_instantiations_expr); }
                    }
                }
                for iv in ivars {
                    if let AstDeclData::Ivar { ivar_type, .. } = &iv.data {
                        if let Some(t) = ivar_type {
                            if !t.type_args.is_empty() {
                                generic_instantiations.push((t.name.clone().unwrap_or_default(), t.type_args.clone()));
                            }
                        }
                    }
                }
            }
        }
        if d.kind == AstDeclKind::Function {
            if let AstDeclData::Function { body, .. } = &d.data {
                if let Some(b) = body { walk_stmt_for_inst(b, &mut generic_instantiations, &collect_instantiations_expr); }
            }
        }
    }
    // Deduplicate instantiations by (base, rendered args).
    generic_instantiations.dedup_by(|a, b| {
        a.0 == b.0 && a.1.iter().map(ast_type_to_c_str).collect::<String>() == b.1.iter().map(ast_type_to_c_str).collect::<String>()
    });
    // Clone generic classes per instantiation with substituted types.
    // Substitute T → concrete type in ivar_types/method return types/params by
    // string replacement (rendered C strings use `NPObject *` for TypePrim::Param).
    for (base, args) in &generic_instantiations {
        let mangled = format!("{}<{}>", base, args.iter().map(ast_type_to_c_str).collect::<Vec<_>>().join(", "));
        let mangled_flat = name_flat(&mangled);
        if class_infos.contains_key(&mangled_flat) { continue; }
        // Find the generic class template to clone (by base name, flat).
        let base_flat = name_flat(base);
        if let Some(template) = class_infos.get(&base_flat).cloned() {
            // Substitute T (rendered as `NPObject *`) → concrete arg type string.
            let concrete_str = args.iter().map(ast_type_to_c_str).collect::<Vec<_>>().join(", ");
            let mut sub_info = template.clone();
            sub_info.class_name = mangled.clone();
            sub_info.flat = mangled_flat.clone();
            // Substitute in ivar_types.
            sub_info.ivar_types = sub_info.ivar_types.iter().map(|s| s.replace("NPObject *", &concrete_str)).collect();
            // Substitute in method_return_types.
            sub_info.method_return_types = sub_info.method_return_types.iter().map(|s| s.replace("NPObject *", &concrete_str)).collect();
            // Substitute in method_params_list (each param type).
            sub_info.method_params_list = sub_info.method_params_list.iter().map(|params| {
                params.iter().map(|(pt, pn)| (pt.replace("NPObject *", &concrete_str), pn.clone())).collect()
            }).collect();
            sub_info.method_owners = sub_info.method_owners.iter().map(|_| mangled_flat.clone()).collect();
            for (i, mname) in sub_info.method_names.iter().enumerate() {
                let fn_name = format!("{}_{}", mangled_flat, mname);
                let ret_type = sub_info.method_return_types.get(i).cloned().unwrap_or_else(|| "NPObject *".into());
                let params = sub_info.method_params_list.get(i).cloned().unwrap_or_default();
                let base_fn_name = format!("{}_{}", base_flat, mname);
                let body = sub_info.method_bodies.get(i)
                    .and_then(|b| b.as_ref().map(|b| Box::new(b.as_ref().clone())))
                    .or_else(|| {
                        decls.iter().find(|d| d.name == base_fn_name).and_then(|d| {
                            if let CgDeclData::Function { ref body, .. } = d.data { body.clone() } else { None }
                        })
                    });
                decls.push(CgDecl {
                    kind: CgDeclKind::Function, name: fn_name,
                    data: CgDeclData::Function {
                        return_type: ret_type,
                        params,
                        body,
                        is_variadic: false,
                        is_objc_class: true,
                    },
                });
            }
            class_infos.insert(mangled_flat, sub_info);
        }
    }
    // Helper to walk statements for expressions containing instantiations.
    // Use `&dyn Fn` (not generic `<F>`) so recursion goes through one shared
    // monomorphization instead of nesting generic instances infinitely
    // (the `<F>` form hit rustc's recursion limit while instantiating).
    type InstList = Vec<(String, Vec<AstType>)>;
    type InstCollector<'a> = &'a dyn Fn(&AstExpr, &mut InstList);
    fn walk_stmt_for_inst(s: &AstStmt, out: &mut InstList, f: InstCollector) {
        match &s.data {
            AstStmtData::Expr(e) => { walk_expr_for_inst(e, out, f); }
            AstStmtData::Decl(d) => { walk_decl_for_inst(d, out, f); }
            AstStmtData::If { then, else_, .. } => {
                walk_stmt_for_inst(then, out, f);
                if let Some(eb) = else_ { walk_stmt_for_inst(eb, out, f); }
            }
            AstStmtData::While { body, .. } | AstStmtData::Do { body, .. } => { walk_stmt_for_inst(body, out, f); }
            AstStmtData::For { init, cond, incr, body, .. } => {
                if let Some(i) = init { walk_stmt_for_inst(i, out, f); }
                if let Some(c) = cond { walk_expr_for_inst(c, out, f); }
                if let Some(u) = incr { walk_expr_for_inst(u, out, f); }
                walk_stmt_for_inst(body, out, f);
            }
            AstStmtData::Compound(stmts) => { for st in stmts { walk_stmt_for_inst(st, out, f); } }
            AstStmtData::Autoreleasepool(body) => { walk_stmt_for_inst(body, out, f); }
            _ => {}
        }
    }
    fn walk_expr_for_inst(e: &AstExpr, out: &mut InstList, f: InstCollector) {
        f(e, out);
        match &e.data {
            AstExprData::FuncCall { args, .. } => {
                for a in args { walk_expr_for_inst(a, out, f); }
            }
            AstExprData::MsgSend { receiver, args, .. } => {
                walk_expr_for_inst(receiver, out, f);
                for a in args { walk_expr_for_inst(a, out, f); }
            }
            _ => {}
        }
    }
    fn walk_decl_for_inst(d: &AstDecl, out: &mut InstList, f: InstCollector) {
        match &d.data {
            AstDeclData::Function { body, .. } => { if let Some(b) = body { walk_stmt_for_inst(b, out, f); } }
            AstDeclData::Variable { init, .. } => { if let Some(i) = init { walk_expr_for_inst(i, out, f); } }
            _ => {}
        }
    }

    for (flat, info) in std::mem::take(&mut class_infos) {
        classes.push(CgClassMeta {
            class_name: info.class_name,
            super_name: info.super_name.clone(),
            method_names: info.method_names,
            is_class_methods: info.is_class_methods,
            method_return_types: info.method_return_types,
            method_params_list: info.method_params_list,
            method_owners: info.method_owners,
            vtable_indices: Vec::new(),
            ivar_types: info.ivar_types,
            ivar_names: info.ivar_names,
            properties: Vec::new(),
        });
    }

    // Sort classes by inheritance depth so parents are processed before children.
    // This ensures that when a class inherits from its parent, the parent's method
    // list already includes its own inherited methods (from grandparents).
    {
        let n = classes.len();
        let mut depth = vec![0usize; n];
        let mut changed = true;
        while changed {
            changed = false;
            for i in 0..n {
                if let Some(ref sup) = classes[i].super_name {
                    if let Some(sup_idx) = classes.iter().position(|c| c.class_name == *sup) {
                        let d = depth[sup_idx] + 1;
                        if d > depth[i] {
                            depth[i] = d;
                            changed = true;
                        }
                    }
                }
            }
        }
        // Bubble sort by depth (stable not required, but simple)
        for i in 0..n {
            for j in i+1..n {
                if depth[j] < depth[i] {
                    classes.swap(i, j);
                    depth.swap(i, j);
                }
            }
        }
    }

    for i in 0..classes.len() {
        let (sup_name, mut method_names, mut is_class_methods, mut method_return_types, mut method_params_list, mut method_owners) = {
            let cm = &classes[i];
            (cm.super_name.clone(), cm.method_names.clone(), cm.is_class_methods.clone(), cm.method_return_types.clone(), cm.method_params_list.clone(), cm.method_owners.clone())
        };
        if let Some(ref sup) = sup_name {
            if let Some(sup_idx) = classes.iter().position(|c| c.class_name == *sup) {
                let sup = &classes[sup_idx];
                // Build complete inherited vtable layout: all parent methods in parent order,
                // with overridden methods replaced by subclass versions at the inherited positions.
                let mut new_names: Vec<String> = Vec::new();
                let mut new_ic: Vec<bool> = Vec::new();
                let mut new_rt: Vec<String> = Vec::new();
                let mut new_pl: Vec<Vec<(String, String)>> = Vec::new();
                let mut new_ow: Vec<String> = Vec::new();
                for (j, mname) in sup.method_names.iter().enumerate() {
                    new_names.push(mname.clone());
                    new_ic.push(sup.is_class_methods[j]);
                    new_rt.push(sup.method_return_types.get(j).cloned().unwrap_or_default());
                    new_pl.push(sup.method_params_list.get(j).cloned().unwrap_or_default());
                    let owner = sup.method_owners.get(j).cloned().unwrap_or_else(|| sup.class_name.clone());
                    new_ow.push(owner.clone());
                    // If subclass overrides, replace with subclass version
                    if let Some(own_idx) = method_names.iter().position(|n| n == mname) {
                        let len = new_names.len();
                        new_ic[len - 1] = is_class_methods[own_idx];
                        new_rt[len - 1] = method_return_types.get(own_idx).cloned().unwrap_or_default();
                        new_pl[len - 1] = method_params_list.get(own_idx).cloned().unwrap_or_default();
                        new_ow[len - 1] = method_owners.get(own_idx).cloned().unwrap_or_else(|| owner);
                    }
                }
                // Append subclass-only methods (not in parent)
                for (j, mname) in method_names.iter().enumerate() {
                    if !sup.method_names.contains(mname) {
                        new_names.push(mname.clone());
                        new_ic.push(is_class_methods[j]);
                        new_rt.push(method_return_types.get(j).cloned().unwrap_or_default());
                        new_pl.push(method_params_list.get(j).cloned().unwrap_or_default());
                        new_ow.push(method_owners.get(j).cloned().unwrap_or_default());
                    }
                }
                method_names = new_names;
                is_class_methods = new_ic;
                method_return_types = new_rt;
                method_params_list = new_pl;
                method_owners = new_ow;
            }
        }
        classes[i].method_names = method_names;
        classes[i].is_class_methods = is_class_methods;
        classes[i].method_return_types = method_return_types;
        classes[i].method_params_list = method_params_list;
        classes[i].method_owners = method_owners;
    }

    CgUnit { decls, filename: ast.filename.clone(), c_headers: Vec::new(), selectors, classes }
}

// ─── C code emission ─────────────────────────────────────────────────────────

pub fn emit_expr(e: &CgExpr, out: &mut String) {
    match &e.data {
        CgExprData::Int(val) => { let _ = write!(out, "{}", val); }
        CgExprData::Float(val) => {
            let s = format!("{}", val);
            if s.contains('.') || s.contains('e') || s.contains('E') {
                let _ = write!(out, "{}f", s);
            } else {
                let _ = write!(out, "{}.0f", s);
            }
        }
        CgExprData::String(s) => { let _ = write!(out, "\"{}\"", s); }
        CgExprData::Char(val) => {
            let escaped = match *val {
                10 => "\\n".to_string(),
                9 => "\\t".to_string(),
                13 => "\\r".to_string(),
                92 => "\\\\".to_string(),
                39 => "\\'".to_string(),
                34 => "\\\"".to_string(),
                c if (32..=126).contains(&c) => format!("{}", c as char),
                c => format!("\\x{:02X}", c),
            };
            let _ = write!(out, "'{}'", escaped);
        }
        CgExprData::Ident(s) => { out.push_str(s); }
        CgExprData::Unary { op_str, operand, is_postfix } => {
            if *is_postfix {
                emit_expr(operand, out);
                out.push_str(op_str);
            } else if op_str == "sizeof" {
                // sizeof needs parentheses around its operand for correct precedence
                out.push_str("sizeof(");
                emit_expr(operand, out);
                out.push(')');
            } else {
                out.push_str(op_str);
                emit_expr(operand, out);
            }
        }
        CgExprData::Binary { op_str, left, right } => {
            out.push('(');
            emit_expr(left, out);
            out.push(' ');
            out.push_str(op_str);
            out.push(' ');
            emit_expr(right, out);
            out.push(')');
        }
        CgExprData::Assign { target, value } => {
            emit_expr(target, out);
            out.push_str(" = ");
            emit_expr(value, out);
        }
        CgExprData::Cast { target_type, expr } => {
            let _ = write!(out, "({})", target_type);
            emit_expr(expr, out);
        }
        CgExprData::Call { name, args, vtable_class, alt_vtable_classes, is_class_method, is_super, sel_const_name } => {
            if *is_super {
                // Super call: direct function call to superclass implementation
                let cls = vtable_class.as_deref().map(|c| name_flat(c)).unwrap_or_else(|| "NPObject".to_string());
                let _ = write!(out, "{}_{}(", cls, name);
                if !args.is_empty() {
                    emit_expr(&args[0], out);
                    let _ = write!(out, ", {}", sel_const_name.as_deref().unwrap_or("0"));
                    for arg in &args[1..] {
                        out.push_str(", ");
                        emit_expr(arg, out);
                    }
                }
                out.push(')');
            } else if let Some(vc) = vtable_class {
                let vc_flat = name_flat(vc);
                if *is_class_method {
                    // Class method: direct function call
                    let _ = write!(out, "{}_{}(", vc_flat, name);
                    if !args.is_empty() {
                        emit_expr(&args[0], out);
                        let _ = write!(out, ", {}", sel_const_name.as_deref().unwrap_or("0"));
                        for arg in &args[1..] {
                            out.push_str(", ");
                            emit_expr(arg, out);
                        }
                    }
                    out.push(')');
                } else {
                    let is_multi = !alt_vtable_classes.is_empty();
                    let all_classes: Vec<&str> = if is_multi {
                        let mut result = vec![vc.as_str()];
                        result.extend(alt_vtable_classes.iter().map(|s| s.as_str()));
                        result
                    } else {
                        vec![vc.as_str()]
                    };
                    let sel = sel_const_name.as_deref().unwrap_or("0");
                    if is_multi {
                        // Multi-dispatch: check isa pointer at runtime
                        let is_simple = args.first().map_or(false, |a| matches!(a.kind, CgExprKind::Ident));
                        if is_simple && !args.is_empty() {
                            // Simple ident receiver — no temp needed
                            for (i, cls) in all_classes.iter().enumerate() {
                                let cf = name_flat(cls);
                                if i == 0 {
                                    let _ = write!(out, "((NPObject *)");
                                    emit_expr(&args[0], out);
                                    let _ = write!(out, ")->isa->vtable == &nupa_{}_vtable_inst ? ", cf);
                                    let _ = write!(out, "((struct nupa_{}_vtable *)((NPObject *)(", cf);
                                    emit_expr(&args[0], out);
                                    let _ = write!(out, "))->isa->vtable)->{}(", name);
                                } else if i < all_classes.len() - 1 {
                                    let _ = write!(out, " : ((NPObject *)");
                                    emit_expr(&args[0], out);
                                    let _ = write!(out, ")->isa->vtable == &nupa_{}_vtable_inst ? ", cf);
                                    let _ = write!(out, "((struct nupa_{}_vtable *)((NPObject *)(", cf);
                                    emit_expr(&args[0], out);
                                    let _ = write!(out, "))->isa->vtable)->{}(", name);
                                } else {
                                    let _ = write!(out, " : ((struct nupa_{}_vtable *)((NPObject *)(", cf);
                                    emit_expr(&args[0], out);
                                    let _ = write!(out, "))->isa->vtable)->{}(", name);
                                }
                                emit_expr(&args[0], out);
                                let _ = write!(out, ", {}", sel);
                                for arg in &args[1..] {
                                    out.push_str(", ");
                                    emit_expr(arg, out);
                                }
                                out.push(')');
                            }
                        } else {
                            // Complex receiver: use temp variable
                            let tid = next_temp_id();
                            let _ = write!(out, "({{ NPObject *__nupa_tmp_{} = (", tid);
                            if !args.is_empty() { emit_expr(&args[0], out); } else { out.push_str("0"); }
                            let _ = write!(out, "); !__nupa_tmp_{} ? 0", tid);
                            for (i, cls) in all_classes.iter().enumerate() {
                                let cf = name_flat(cls);
                                let r = format!("__nupa_tmp_{}", tid);
                                if i == 0 {
                                    let _ = write!(out, " : {}->isa->vtable == &nupa_{}_vtable_inst ? ", r, cf);
                                } else if i < all_classes.len() - 1 {
                                    let _ = write!(out, " : {}->isa->vtable == &nupa_{}_vtable_inst ? ", r, cf);
                                } else {
                                    out.push_str(" : ");
                                }
                                let _ = write!(out, "((struct nupa_{}_vtable *){}->isa->vtable)->{}(", cf, r, name);
                                let _ = write!(out, "{}, {}", r, sel);
                                for arg in &args[1..] {
                                    out.push_str(", ");
                                    emit_expr(arg, out);
                                }
                                out.push(')');
                            }
                            out.push_str("; })");
                        }
                    } else {
                        // Single class vtable dispatch (original logic preserved)
                        let is_simple = args.first().map_or(false, |a| matches!(a.kind, CgExprKind::Ident));
                        let needs_temp = !is_simple && !args.is_empty();
                        let vc_flat = name_flat(vc);
                        if needs_temp {
                            let tid = next_temp_id();
                            let _ = write!(out, "({{ NPObject *__nupa_tmp_{} = (", tid);
                            emit_expr(&args[0], out);
                            let _ = write!(out, "); __nupa_tmp_{} ? ((struct nupa_{}_vtable *)__nupa_tmp_{}->isa->vtable)->{}(", tid, vc_flat, tid, name);
                            let _ = write!(out, "__nupa_tmp_{}", tid);
                            let _ = write!(out, ", {}", sel);
                            for arg in &args[1..] {
                                out.push_str(", ");
                                emit_expr(arg, out);
                            }
                            out.push_str(") : 0; })");
                        } else {
                            let _ = write!(out, "((struct nupa_{}_vtable *)((NPObject *)(", vc_flat);
                            if !args.is_empty() { emit_expr(&args[0], out); } else { out.push_str("0"); }
                            out.push_str("))->isa->vtable)");
                            let _ = write!(out, "->{}(", name);
                            if !args.is_empty() {
                                emit_expr(&args[0], out);
                                let _ = write!(out, ", {}", sel);
                                for arg in &args[1..] {
                                    out.push_str(", ");
                                    emit_expr(arg, out);
                                }
                            }
                            out.push(')');
                        }
                    }
                }
            } else if name == "autorelease" {
                // autorelease is a no-op in Nupa's non-ARC runtime; just return receiver
                if !args.is_empty() { emit_expr(&args[0], out); }
            } else {
                // Direct C function call
                // If sel_const_name is set, this is a message send that wasn't found in the vtable.
                // Emit as arrow access: receiver->method (ivar/property access)
                if sel_const_name.is_some() && !args.is_empty() {
                    emit_expr(&args[0], out);
                    let _ = write!(out, "->{}", name);
                } else {
                    out.push_str(name);
                    out.push('(');
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 { out.push_str(", "); }
                        emit_expr(arg, out);
                    }
                    out.push(')');
                }
            }
        }
        CgExprData::Comma(items) => {
            out.push('(');
            for (i, item) in items.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                emit_expr(item, out);
            }
            out.push(')');
        }
        CgExprData::Member { obj, field } => {
            emit_expr(obj, out);
            let _ = write!(out, ".{}", field);
        }
        CgExprData::Arrow { obj, field } => {
            if obj.kind == CgExprKind::Cast {
                out.push('(');
            }
            emit_expr(obj, out);
            if obj.kind == CgExprKind::Cast {
                out.push(')');
            }
            let _ = write!(out, "->{}", field);
        }
        CgExprData::Index { arr, index } => {
            emit_expr(arr, out);
            out.push('[');
            emit_expr(index, out);
            out.push(']');
        }
        CgExprData::Ternary { cond, then, else_ } => {
            emit_expr(cond, out);
            out.push_str(" ? ");
            emit_expr(then, out);
            out.push_str(" : ");
            emit_expr(else_, out);
        }
        CgExprData::InitList(items) => {
            out.push_str("{ ");
            for (i, item) in items.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                emit_expr(item, out);
            }
            out.push_str(" }");
        }
        CgExprData::Sizeof(type_str) => {
            let _ = write!(out, "sizeof({})", type_str);
        }
        CgExprData::BlockLit(data) => {
            // Emit block literal: ^return_type(params) { body }
            let rt = &data.return_type;
            out.push('^');
            out.push_str(rt);
            out.push('(');
            for (i, (pt, pn)) in data.params.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                out.push_str(pt);
                out.push(' ');
                out.push_str(pn);
            }
            out.push(')');
            if let Some(ref body) = data.body {
                out.push(' ');
                emit_stmt(body, out, 0);
            } else {
                out.push_str(" {}");
            }
        }
    }
}

pub fn emit_stmt(s: &CgStmt, out: &mut String, indent: usize) {
    let ind = "  ".repeat(indent);
    match &s.data {
        CgStmtData::Expr(e) => {
            out.push_str(&ind);
            emit_expr(e, out);
            out.push_str(";\n");
        }
        CgStmtData::Compound(stmts) => {
            out.push_str(&ind);
            out.push_str("{\n");
            for st in stmts {
                emit_stmt(st, out, indent + 1);
            }
            out.push_str(&ind);
            out.push_str("}\n");
        }
        CgStmtData::If { cond, then, else_ } => {
            out.push_str(&ind);
            out.push_str("if (");
            emit_expr(cond, out);
            out.push_str(") ");
            emit_stmt(then, out, indent);
            if let Some(el) = else_ {
                out.push_str(&ind);
                out.push_str("else ");
                emit_stmt(el, out, indent);
            }
        }
        CgStmtData::While { cond, body } => {
            out.push_str(&ind);
            out.push_str("while (");
            emit_expr(cond, out);
            out.push_str(") ");
            emit_stmt(body, out, indent);
        }
        CgStmtData::Do { body, cond } => {
            out.push_str(&ind);
            out.push_str("do ");
            emit_stmt(body, out, indent);
            out.push_str(&ind);
            out.push_str("while (");
            emit_expr(cond, out);
            out.push_str(");\n");
        }
        CgStmtData::For { init, cond, incr, body } => {
            out.push_str(&ind);
            out.push_str("for (");
            if let Some(i) = init { emit_stmt(i, out, 0); }
            else { out.push_str("; "); }
            if let Some(c) = cond { emit_expr(c, out); }
            out.push_str("; ");
            if let Some(i) = incr { emit_expr(i, out); }
            out.push_str(") ");
            emit_stmt(body, out, indent);
        }
        CgStmtData::Return(value) => {
            out.push_str(&ind);
            out.push_str("return");
            if let Some(v) = value {
                out.push(' ');
                emit_expr(v, out);
            }
            out.push_str(";\n");
        }
        CgStmtData::Break => { out.push_str(&ind); out.push_str("break;\n"); }
        CgStmtData::Continue => { out.push_str(&ind); out.push_str("continue;\n"); }
        CgStmtData::Goto(label) => { let _ = write!(out, "{}goto {};\n", ind, label); }
        CgStmtData::Label(name) => { let _ = write!(out, "{}:\n", name); }
        CgStmtData::Switch { expr, body } => {
            out.push_str(&ind);
            out.push_str("switch (");
            emit_expr(expr, out);
            out.push_str(") ");
            emit_stmt(body, out, indent);
        }
        CgStmtData::Case { value, body } => {
            out.push_str(&ind);
            out.push_str("case ");
            emit_expr(value, out);
            out.push_str(":\n");
            emit_stmt(body, out, indent + 1);
        }
        CgStmtData::Default(body) => {
            out.push_str(&ind);
            out.push_str("default:\n");
            emit_stmt(body, out, indent + 1);
        }
        CgStmtData::Decl { decl_type, name, init, array_suffix, is_static, is_weak, next } => {
            // Detect alloc+init pattern: vtable dispatch call where the receiver is a complex expression
            // (like another function call). Emit a temp variable to avoid double evaluation.
            let should_split = init.as_ref().map_or(false, |i| {
                if let CgExprData::Call { args, .. } = &i.data {
                    args.first().map_or(false, |a| matches!(a.kind, CgExprKind::Call))
                } else { false }
            });
            if should_split {
                let decl_var_name = name; // Save the declaration variable name before shadowing
                let init_data = init.as_ref().and_then(|i| {
                    if let CgExprData::Call { ref name, ref args, vtable_class: Some(ref vc), is_class_method, is_super, ref sel_const_name, .. } = i.data {
                        Some((name.clone(), args.clone(), vc.clone(), is_class_method, is_super, sel_const_name.clone()))
                    } else { None }
                });
                if let Some((ref method_name, ref args, ref vc, is_class_method, is_super, sel_const_name)) = init_data {
                    let vc_flat = name_flat(vc);
                    out.push_str(&ind);
                    if !args.is_empty() {
                            let tid = next_temp_id();
                            // Emit: NPObject *__nupa_tmp_N = receiver;
                            let _ = write!(out, "NPObject *__nupa_tmp_{} = (", tid);
                            emit_expr(&args[0], out);
                            out.push_str(");\n");
                            out.push_str(&ind);
                            if *is_static { out.push_str("static "); }
                            // Emit: type name = ((vtable *)__nupa_tmp_N->isa->vtable)->method(__nupa_tmp_N, sel, ...);
                            out.push_str(decl_type);
                            out.push(' ');
                            out.push_str(decl_var_name); // the variable name from the declaration
                            if let Some(suffix) = array_suffix { out.push_str(suffix); }
                            let _ = write!(out, " = ((struct nupa_{}_vtable *)__nupa_tmp_{}->isa->vtable)->", vc_flat, tid);
                            out.push_str(method_name); // the method name
                            let _ = write!(out, "(__nupa_tmp_{}", tid);
                        let _ = write!(out, ", {}", sel_const_name.as_deref().unwrap_or("0"));
                        for arg in &args[1..] {
                            out.push_str(", ");
                            emit_expr(arg, out);
                        }
                        out.push_str(");\n");
                    } else {
                        if *is_static { out.push_str("static "); }
                        out.push_str(decl_type);
                        out.push(' ');
                        out.push_str(name);
                        if let Some(suffix) = array_suffix { out.push_str(suffix); }
                        out.push_str(" = ((struct nupa_");
                        out.push_str(&vc_flat);
                        out.push_str("_vtable *)0)->");
                        out.push_str(name);
                        out.push_str("(");
                        let _ = write!(out, "{}", sel_const_name.as_deref().unwrap_or("0"));
                        out.push_str(");\n");
                    }
                } else {
                    // Fallback (shouldn't reach here)
                    out.push_str(&ind);
                    out.push_str(decl_type);
                    out.push(' ');
                    out.push_str(name);
                    if let Some(suffix) = array_suffix { out.push_str(suffix); }
                    if let Some(i) = init { out.push_str(" = "); emit_expr(i, out); }
                    for (n_name, n_init) in next {
                        out.push_str(", ");
                        out.push_str(n_name);
                        if let Some(i) = n_init {
                            out.push_str(" = ");
                            emit_expr(i, out);
                        }
                    }
                    out.push_str(";\n");
                }
            } else {
                out.push_str(&ind);
                if *is_static { out.push_str("static "); }
                // For block types like `int (^name)(int)`, the variable name is already inside the type
                let is_block_type = decl_type.contains("(^");
                if is_block_type {
                    out.push_str(decl_type);
                } else {
                    out.push_str(decl_type);
                    if *is_weak { out.push_str(" __attribute__((cleanup(nupa_weak_auto_cleanup)))"); }
                    out.push(' ');
                    out.push_str(name);
                }
                if let Some(suffix) = array_suffix { out.push_str(suffix); }
                if let Some(i) = init {
                    out.push_str(" = ");
                    emit_expr(i, out);
                }
                for (n_name, n_init) in next {
                    out.push_str(", ");
                    out.push_str(n_name);
                    if let Some(i) = n_init {
                        out.push_str(" = ");
                        emit_expr(i, out);
                    }
                }
                out.push_str(";\n");
                if *is_weak {
                    if let Some(ref init_expr) = init {
                        let _ = write!(out, "{}nupa_weak_register((NPObject **)&{}, (NPObject *)", ind, name);
                        emit_expr(init_expr, out);
                        out.push_str(");\n");
                    }
                }
            }
        }
        CgStmtData::Empty => {}
        CgStmtData::ForIn { var_name, collection, body } => {
            out.push_str(&ind);
            let _ = write!(out, "{{ size_t _count = nupa_array_count(");
            emit_expr(collection, out);
            out.push_str(");\n");
            let _ = write!(out, "{}for (size_t _i = 0; _i < _count; _i++) {{\n", ind);
            let _ = write!(out, "{}    ", ind);
            out.push_str(var_name);
            out.push_str(" = ((");
            // type from collection element
            out.push_str("typeof(");
            emit_expr(collection, out);
            out.push_str("[0])");
            out.push_str(")");
            emit_expr(collection, out);
            out.push_str("[_i];\n");
            emit_stmt(body, out, indent + 2);
            let _ = write!(out, "{}}}\n", ind);
            let _ = write!(out, "{}}}\n", ind);
        }
    }
}

pub fn emit_decl(d: &CgDecl, out: &mut String) {
    match &d.data {
        CgDeclData::Function { return_type, params, body, .. } => {
            out.push_str(return_type);
            out.push(' ');
            out.push_str(&d.name);
            out.push('(');
            for (i, (pt, pn)) in params.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                out.push_str(&format_param_decl(pt, pn));
            }
            if params.is_empty() { out.push_str("void"); }
            out.push(')');
            if let Some(b) = body {
                out.push(' ');
                emit_stmt(b, out, 0);
            } else {
                out.push_str(";\n");
            }
        }
        CgDeclData::Variable { var_type, init, is_static, is_const, next, .. } => {
            if *is_static { out.push_str("static "); }
            if *is_const { out.push_str("const "); }
            // For block types like `int (^name)(int)`, the variable name is already inside the type
            let is_block_type = var_type.contains("(^");
            if is_block_type {
                // Block type already includes the variable name, just append initializer
                out.push_str(var_type);
                if let Some(i) = init {
                    out.push_str(" = ");
                    emit_expr(i, out);
                }
                out.push_str(";\n");
            } else {
                out.push_str(var_type);
                out.push(' ');
                out.push_str(&d.name);
                if let Some(i) = init {
                    out.push_str(" = ");
                    emit_expr(i, out);
                }
                for (n_name, n_init) in next {
                    out.push_str(", ");
                    out.push_str(n_name);
                    if let Some(i) = n_init {
                        out.push_str(" = ");
                        emit_expr(i, out);
                    }
                }
                out.push_str(";\n");
            }
        }
        CgDeclData::Typedef { alias, type_str, struct_fields } => {
            // Struct typedefs with struct_fields are already emitted at the
            // forward-declaration point (before function declarations). Skip
            // them here to avoid duplicates.
            if struct_fields.is_empty() {
                let _ = write!(out, "typedef {} {};\n", type_str, alias);
            }
        }
        CgDeclData::Struct { fields } => {
            let _ = write!(out, "struct {} {{\n", d.name);
            for (ft, fn_) in fields {
                let _ = write!(out, "    {} {};\n", ft, fn_);
            }
            out.push_str("};\n");
        }
        CgDeclData::ExternFunc { return_type, params, .. } => {
            out.push_str(return_type);
            out.push(' ');
            out.push_str(&d.name);
            out.push('(');
            for (i, (pt, pn)) in params.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                out.push_str(pt);
                out.push(' ');
                out.push_str(pn);
            }
            if params.is_empty() { out.push_str("void"); }
            out.push_str(");\n");
        }
        CgDeclData::Enum { members } => {
            let _ = write!(out, "enum {} {{\n", d.name);
            for (i, (m, v)) in members.iter().enumerate() {
                if i > 0 { out.push_str(",\n"); }
                let _ = write!(out, "    {} = {}", m, v);
            }
            out.push_str("\n};\n");
            // typedef alias so that `GameState var;` (source uses typedef-enum
            // form `typedef enum {...} GameState;`) resolves. Without this,
            // C only sees `enum GameState` but not the bare `GameState` name,
            // causing `unknown type name 'GameState'`.
            let _ = write!(out, "typedef enum {} {};\n", d.name, d.name);
        }
    }
}

pub fn emit_unit_with_headers(unit: &CgUnit, c_headers: &[String], search_dirs: &[String]) -> String {
    let mut out = String::new();
    out.push_str("// Generated by nupac\n");
    for h in c_headers {
        out.push_str(h);
        out.push('\n');
    }

    // Scan C headers for structs already defined externally
    let mut header_structs: std::collections::HashSet<String> = std::collections::HashSet::new();
    for directive in c_headers {
        if let Some(rest) = directive.strip_prefix("#include ") {
            let rest = rest.trim();
            let path = if rest.starts_with('"') && rest.ends_with('"') {
                &rest[1..rest.len()-1]
            } else if rest.starts_with('<') && rest.ends_with('>') {
                &rest[1..rest.len()-1]
            } else {
                continue;
            };
            for dir in search_dirs {
                let full = format!("{}/{}", dir, path);
                if let Ok(content) = std::fs::read_to_string(&full) {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if trimmed.starts_with("struct ") && trimmed.contains('{') {
                            if let Some(name_rest) = trimmed.strip_prefix("struct ") {
                                let name = name_rest.split('{').next().unwrap_or(name_rest).trim();
                                if !name.is_empty() {
                                    header_structs.insert(name.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Forward-declare vtable structs
    for cm in &unit.classes {
        let flat_cn = name_flat(&cm.class_name);
        let has_methods = !cm.method_names.is_empty();
        let has_super = cm.super_name.is_some();
        let has_class_methods = cm.is_class_methods.iter().any(|&c| c);
        if has_methods || has_super {
            let _ = write!(out, "struct nupa_{}_vtable;\n", flat_cn);
            if has_class_methods || has_super {
                let _ = write!(out, "struct nupa_{}_meta_vtable;\n", flat_cn);
            }
        }
    }
    if !unit.classes.is_empty() { out.push('\n'); }

    // SEL constants
    for sel in &unit.selectors {
        let h = fnv1a_hash(sel);
        let sn = sel_const_name(sel);
        let _ = write!(out, "static const SEL {} = {{.name = \"{}\", .hash = 0x{:08X}}};\n", sn, sel, h);
    }
    if !unit.selectors.is_empty() { out.push('\n'); }

    // Typedefs for class types
    for cm in &unit.classes {
        let fc = name_flat(&cm.class_name);
        let _ = write!(out, "typedef struct {} {};\n", fc, fc);
    }
    if !unit.classes.is_empty() { out.push('\n'); }

    // Forward declarations for enums (must come BEFORE struct/class metadata
    // so that `GameState var;` inside an ivar list resolves. Without this,
    // the enum decl is emitted near the end of the file (after structs that
    // reference it), causing `unknown type name 'GameState'` errors.
    for decl in &unit.decls {
        if let CgDeclData::Enum { members } = &decl.data {
            let _ = write!(out, "enum {} {{\n", decl.name);
            for (i, (m, v)) in members.iter().enumerate() {
                if i > 0 { out.push_str(",\n"); }
                let _ = write!(out, "    {} = {}", m, v);
            }
            out.push_str("\n};\n");
            let _ = write!(out, "typedef enum {} {};\n", decl.name, decl.name);
        }
    }
    if unit.decls.iter().any(|d| matches!(d.data, CgDeclData::Enum { .. })) { out.push('\n'); }

    // Forward declarations for typedefs (must come before function declarations).
    // For non-struct typedefs (`typedef int MyInt`) emit the bare typedef.
    // For struct typedefs (`typedef struct { ... } Name`) emit the full struct
    // definition here rather than a forward declaration, since anonymous structs
    // (`typedef struct { ... } Name`) have no struct tag to forward-declare.
    for decl in &unit.decls {
        if let CgDeclData::Typedef { ref alias, ref type_str, ref struct_fields } = decl.data {
            if struct_fields.is_empty() {
                let _ = write!(out, "typedef {} {};\n", type_str, alias);
            } else {
                out.push_str("typedef struct ");
                out.push_str(alias);
                out.push_str(" {\n");
                for (ft, fn_) in struct_fields {
                    let _ = write!(out, "    {} {};\n", ft, fn_);
                }
                out.push_str("} ");
                out.push_str(alias);
                out.push_str(";\n");
            }
        }
    }
    if unit.decls.iter().any(|d| matches!(d.data, CgDeclData::Typedef { .. })) { out.push('\n'); }

    // Forward declarations for functions
    for decl in &unit.decls {
        if let CgDeclData::Function { ref return_type, ref params, is_variadic, .. } = decl.data {
            let _ = write!(out, "{} {}(", return_type, decl.name);
            for (i, (pt, pn)) in params.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                out.push_str(&format_param_decl(pt, pn));
            }
            if params.is_empty() { out.push_str("void"); }
            if is_variadic { out.push_str(", ..."); }
            out.push_str(");\n");
        }
    }
    if !unit.decls.is_empty() { out.push('\n'); }

    // File-level variable declarations (must precede function definitions)
    for decl in &unit.decls {
        if let CgDeclData::Variable { ref var_type, ref init, is_static, is_const, .. } = decl.data {
            if is_static { out.push_str("static "); }
            if is_const { out.push_str("const "); }
            let is_block_type = var_type.contains("(^");
            if is_block_type {
                out.push_str(var_type);
                if let Some(i) = init {
                    out.push_str(" = ");
                    emit_expr(i, &mut out);
                }
                out.push_str(";\n");
            } else {
                out.push_str(var_type);
                out.push(' ');
                out.push_str(&decl.name);
                if let Some(i) = init {
                    out.push_str(" = ");
                    emit_expr(i, &mut out);
                }
                out.push_str(";\n");
            }
        }
    }
    if unit.decls.iter().any(|d| matches!(d.data, CgDeclData::Variable { .. })) { out.push('\n'); }

    // Forward declarations for getClass functions
    for cm in &unit.classes {
        let has_class_methods = cm.is_class_methods.iter().any(|&c| c);
        let has_super = cm.super_name.is_some();
        if has_class_methods || has_super {
            let fc = name_flat(&cm.class_name);
            let _ = write!(out, "NPClass * {}_getClass(NPClass * self, SEL _cmd);\n", fc);
        }
    }
    if unit.classes.iter().any(|cm| cm.is_class_methods.iter().any(|&c| c) || cm.super_name.is_some()) {
        out.push('\n');
    }

    // Vtable struct definitions
    for cm in &unit.classes {
        if !cm.method_names.is_empty() {
            let fc = name_flat(&cm.class_name);
            let _ = write!(out, "struct nupa_{}_vtable {{\n", fc);
            for (i, (mname, &is_class)) in cm.method_names.iter().zip(&cm.is_class_methods).enumerate() {
                if !is_class {
                    let rt = cm.method_return_types.get(i).map(|s| s.as_str()).unwrap_or("NPObject *");
                    let params = cm.method_params_list.get(i).map(|p| p.iter().map(|(pt, _)| pt.clone()).collect::<Vec<_>>().join(", ")).unwrap_or_else(|| "NPObject *, SEL".to_string());
                    let _ = write!(out, "    {} (*{})({});\n", rt, mname, params);
                }
            }
            out.push_str("};\n");
            let has_class_methods = cm.is_class_methods.iter().any(|&c| c);
            let has_super = cm.super_name.is_some();
            if has_class_methods || has_super {
                let _ = write!(out, "struct nupa_{}_meta_vtable {{\n", name_flat(&cm.class_name));
                for (i, (mname, &is_class)) in cm.method_names.iter().zip(&cm.is_class_methods).enumerate() {
                    if is_class {
                        let rt = cm.method_return_types.get(i).map(|s| s.as_str()).unwrap_or("NPObject *");
                        let params = cm.method_params_list.get(i).map(|p| p.iter().map(|(pt, _)| pt.clone()).collect::<Vec<_>>().join(", ")).unwrap_or_else(|| "NPClass *, SEL".to_string());
                        let _ = write!(out, "    {} (*{})({});\n", rt, mname, params);
                    }
                }
                let _ = write!(out, "    NPClass * (*class)(NPClass *, SEL);\n");
                out.push_str("};\n");
            }
        }
    }
    if !unit.classes.is_empty() { out.push('\n'); }

    // Class struct definitions (skip if already in C headers)
    // Pre-compute a map from flat class name → CgClassMeta so we can walk the
    // superclass chain when emitting a subclass's struct fields. A subclass
    // struct must physically contain the parent's ivars (C has no inheritance),
    // so we emit the superclass ivars first, then this class's own ivars.
    let class_meta_by_name: std::collections::HashMap<String, &CgClassMeta> =
        unit.classes.iter().map(|cm| (name_flat(&cm.class_name), cm)).collect();
    for cm in &unit.classes {
        let skip = header_structs.contains(&cm.class_name);
        if skip { continue; }
        let _ = write!(out, "struct {} {{\n", name_flat(&cm.class_name));
        let _ = write!(out, "    struct NPClass *isa;\n");
        let _ = write!(out, "    uint32_t retain_count;\n");
        // Walk superclass chain and emit ancestor ivars (nearest ancestor first
        // matches NPObject's layout: parent fields precede child fields).
        // Walk superclass chain and emit ancestor ivars (nearest ancestor first
        // matches NPObject's layout: parent fields precede child fields).
        let mut chain: Vec<&CgClassMeta> = Vec::new();
        let mut cur = cm;
        loop {
            if let Some(ref sup) = cur.super_name {
                let sup_flat = name_flat(sup);
                if let Some(sup_cm) = class_meta_by_name.get(&sup_flat) {
                    chain.push(sup_cm);
                    cur = sup_cm;
                    continue;
                }
            }
            break;
        }
        for ancestor in chain.iter().rev() {
            for (ivt, ivn) in ancestor.ivar_types.iter().zip(ancestor.ivar_names.iter()) {
                if let Some(pos) = ivt.find('[') {
                    let decl_t = &ivt[..pos].trim();
                    let suffix = &ivt[pos..];
                    let _ = write!(out, "    {} {}{};\n", decl_t, ivn, suffix);
                } else {
                    let _ = write!(out, "    {} {};\n", ivt, ivn);
                }
            }
        }
        for (ivt, ivn) in cm.ivar_types.iter().zip(cm.ivar_names.iter()) {
            if let Some(pos) = ivt.find('[') {
                let decl_t = &ivt[..pos].trim();
                let suffix = &ivt[pos..];
                let _ = write!(out, "    {} {}{};\n", decl_t, ivn, suffix);
            } else {
                let _ = write!(out, "    {} {};\n", ivt, ivn);
            }
        }
        out.push_str("};\n");
        let _ = write!(out, "typedef struct {} {};\n", name_flat(&cm.class_name), name_flat(&cm.class_name));
        out.push('\n');
    }

    // Forward-declare class metadata variables
    for cm in &unit.classes {
        let _ = write!(out, "extern NPClass nupa_{}_class;\n", name_flat(&cm.class_name));
    }
    if !unit.classes.is_empty() {
        out.push_str("void nupa_meta_init(void);\n\n");
    }

    // Instance vtable instances
    for cm in &unit.classes {
        let instance_entries: Vec<(&String, &String)> = cm.method_names.iter().zip(&cm.is_class_methods).enumerate().filter(|(_, (_, &ic))| !ic).map(|(idx, (n, _))| (n, &cm.method_owners[idx])).collect();
        if instance_entries.is_empty() { continue; }
        let _ = write!(out, "struct nupa_{}_vtable nupa_{}_vtable_inst = {{\n", name_flat(&cm.class_name), name_flat(&cm.class_name));
        for (mname, owner) in &instance_entries {
            let _ = write!(out, "    .{} = {}_{},\n", mname, owner, mname);
        }
        out.push_str("};\n\n");
    }

    // Meta vtable instances
    for cm in &unit.classes {
        let class_entries: Vec<(&String, &String)> = cm.method_names.iter().zip(&cm.is_class_methods).enumerate().filter(|(_, (_, &ic))| ic).map(|(idx, (n, _))| (n, &cm.method_owners[idx])).collect();
        let has_class_methods = !class_entries.is_empty();
        let has_super = cm.super_name.is_some();
        if !has_class_methods && !has_super { continue; }
        let _ = write!(out, "struct nupa_{}_meta_vtable nupa_{}_meta_vtable_inst = {{\n", name_flat(&cm.class_name), name_flat(&cm.class_name));
        for (mname, owner) in &class_entries {
            let _ = write!(out, "    .{} = {}_{},\n", mname, owner, mname);
        }
        let _ = write!(out, "    .class = {}_getClass,\n", name_flat(&cm.class_name));
        out.push_str("};\n\n");
    }

    // getClass implementations for each class
    for cm in &unit.classes {
        let has_class_methods = cm.is_class_methods.iter().any(|&c| c);
        let has_super = cm.super_name.is_some();
        if !has_class_methods && !has_super { continue; }
        let _ = write!(out, "NPClass * {}_getClass(NPClass * self, SEL _cmd) {{\n", name_flat(&cm.class_name));
        out.push_str("    (void)_cmd;\n");
        out.push_str("    return self;\n");
        out.push_str("}\n\n");
    }

    // Class metadata variables
    for cm in &unit.classes {
        let _ = write!(out, "NPClass nupa_{}_class;\n", name_flat(&cm.class_name));
    }
    if !unit.classes.is_empty() { out.push('\n'); }

    // nupa_meta_init()
    if !unit.classes.is_empty() {
        out.push_str("void nupa_meta_init(void) {\n");
        for cm in &unit.classes {
            let _ = write!(out, "    nupa_{}_class = (NPClass){{\n", name_flat(&cm.class_name));
            out.push_str(&format!("        .name = \"{}\",\n", cm.class_name));
            if let Some(ref sup) = cm.super_name {
                if sup == "__nupa_root" {
                    out.push_str("        .superclass = NULL,\n");
                } else {
                    out.push_str(&format!("        .superclass = &nupa_{}_class,\n", name_flat(sup)));
                }
            } else {
                out.push_str("        .superclass = NULL,\n");
            }
            out.push_str(&format!("        .instance_size = sizeof(struct {}),\n", name_flat(&cm.class_name)));
            if cm.method_names.is_empty() {
                if let Some(ref sup) = cm.super_name {
                    if sup == "__nupa_root" {
                        out.push_str("        .vtable = NULL,\n");
                    } else {
                        out.push_str(&format!("        .vtable = &nupa_{}_vtable_inst,\n", name_flat(sup)));
                    }
                } else {
                    out.push_str("        .vtable = NULL,\n");
                }
            } else {
                out.push_str(&format!("        .vtable = &nupa_{}_vtable_inst,\n", name_flat(&cm.class_name)));
            }
            let has_class_methods = cm.is_class_methods.iter().any(|&c| c);
            let has_super = cm.super_name.is_some();
            if !has_class_methods && !has_super {
                out.push_str("        .class_vtable = NULL,\n");
            } else {
                out.push_str(&format!("        .class_vtable = &nupa_{}_meta_vtable_inst,\n", name_flat(&cm.class_name)));
            }
            out.push_str("        .protocol_count = 0,\n");
            out.push_str("    };\n");
        }
        out.push_str("}\n");
    }

    // Function definitions
    for decl in &unit.decls {
        // Skip enum and variable decls — they are already emitted in the
        // forward-declaration phase above. Emitting again here would cause
        // `redefinition` errors.
        if matches!(decl.data, CgDeclData::Enum { .. }) { continue; }
        if matches!(decl.data, CgDeclData::Variable { .. }) { continue; }
        emit_decl(decl, &mut out);
        out.push('\n');
    }

    out
}

pub fn emit_unit(unit: &CgUnit) -> String {
    emit_unit_with_headers(unit, &[], &[])
}