use std::fmt::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::collections::HashMap;
use nupa_ast::*;
use nupa_symbol::*;
use nupa_cst::TypePrim;

// ─── Temp variable counter ─────────────────────────────────────────────────
static TEMP_VAR_COUNTER: AtomicUsize = AtomicUsize::new(0);

// ─── Global vtable method metadata (set during ast_to_cg_unit) ────────────
static METHOD_METADATA: OnceLock<HashMap<String, (usize, String)>> = OnceLock::new();

fn get_vtable_param_type(method_name: &str, param_index: usize) -> Option<String> {
    METHOD_METADATA.get()
        .and_then(|meta| meta.get(method_name))
        .and_then(|(_, ptr_type)| {
            // ptr_type is "return_type (*)(param1, param2, ...)"
            // Block types like "void (^)(FSNode *, _Bool *)" contain commas, so
            // we must split by comma at depth 0 (outside any nested parentheses).
            let paren_start = ptr_type.find("(*)(")?;
            let inner = &ptr_type[paren_start + 4..];
            let paren_end = inner.rfind(')')?;
            let params_str = &inner[..paren_end];
            let mut params: Vec<String> = Vec::new();
            let mut depth: i32 = 0;
            let mut start = 0;
            for (i, ch) in params_str.char_indices() {
                match ch {
                    '(' | '<' => depth += 1,
                    ')' | '>' => depth -= 1,
                    ',' if depth == 0 => {
                        params.push(params_str[start..i].trim().to_string());
                        start = i + 1;
                    }
                    _ => {}
                }
            }
            let last = params_str[start..].trim();
            if !last.is_empty() {
                params.push(last.to_string());
            }
            params.get(param_index).cloned()
        })
}
static BLOCK_TYPEDEF_NAMES: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

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
        method_index: Option<usize>,
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
    Decl { decl_type: String, name: String, init: Option<Box<CgExpr>>, array_suffix: Option<String>, is_static: bool, is_weak: bool, is_block: bool, next: Vec<(String, Option<Box<CgExpr>>)> },
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
        is_block: bool,
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
    pub global_instance_method_names: Vec<String>,
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
    // Split trailing `*` from base so `Node*` → `Node_ptr`, `Node **` → `Node_ptr_ptr`.
    // Anything after the first `*` (including more `*` and whitespace) is pointer levels.
    let (base_ident, base_ptr_levels) = split_trailing_ptrs(&base);
    // Build final: base + "_" + args if any args were rendered
    if out.is_empty() {
        // No generic args — just :: replacement + trailing _ptr
        let mut result = base_ident.replace("::", "__");
        for _ in 0..base_ptr_levels {
            result.push_str("_ptr");
        }
        return result;
    }
    let mut full = base_ident.replace("::", "__");
    full.push('_');
    full.push_str(&out);
    for _ in 0..base_ptr_levels {
        full.push_str("_ptr");
    }
    full
}

/// Split a base string into (identifier_part, trailing_*_count).
/// `Node*` → (`Node*`, 0)  — wait, we want to split *before* the first `*`.
/// Actually: `Node` + count=1; `Node **` → `Node` + 2; `Box` → `Box` + 0.
fn split_trailing_ptrs(base: &str) -> (String, usize) {
    // Find the first `*` position — everything from there is trailing pointers.
    if let Some(pos) = base.find('*') {
        let ident = base[..pos].trim_end().to_string();
        let mut count = 0;
        for ch in base[pos..].chars() {
            if ch == '*' { count += 1; }
        }
        (ident, count)
    } else {
        (base.to_string(), 0)
    }
}

// Mangle a single generic type argument like `QuantumToken *` → `QuantumToken_ptr`.
// Handles nested generics: `Box<Node*>` → `Box_Node_ptr`,
// `Box<Box<Node*>>` → `Box_Box_Node_ptr_ptr`.
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
    // Nested generic args: `Name<...>` → `Name_` + mangled inner args
    if iter.peek() == Some(&'<') {
        iter.next(); // consume '<'
        let flat_base = name.replace("::", "__");
        s.push_str(&flat_base);
        s.push('_');
        // collect inner until matching '>'
        let mut depth = 1;
        let mut inner = String::new();
        while let Some(&c) = iter.peek() {
            if c == '<' { depth += 1; inner.push(c); iter.next(); }
            else if c == '>' {
                depth -= 1;
                if depth == 0 { iter.next(); break; }
                inner.push(c);
                iter.next();
            } else {
                inner.push(c);
                iter.next();
            }
        }
        // inner is like `Node *` or `Box<Node *>` — mangle via name_flat
        s.push_str(&name_flat(&inner));
        // After the closing '>', there may be trailing `*` for pointer levels
        // (already accounted for in the inner name_flat for nested generics,
        // but for a simple `Node *` the trailing `*` is part of inner).
    } else {
        // Simple identifier — render with trailing `*` → `_ptr`
        let flat = name.replace("::", "__");
        s.push_str(&flat);
    }
    // count trailing `*` that are NOT part of the generic args
    while let Some(&c) = iter.peek() {
        if c == '*' { ptr_count += 1; iter.next(); }
        else if c.is_whitespace() { iter.next(); }
        else { break; }
    }
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
                let flat = if ct.type_args.is_empty() {
                    name_flat(name)
                } else {
                    let args_str = ct.type_args.iter()
                        .map(cst_type_to_c_str)
                        .collect::<Vec<_>>()
                        .join(", ");
                    name_flat(&format!("{}<{}>", name, args_str))
                };
                if ct.is_struct { s.push_str(&format!("struct {}", flat)); }
                else { s.push_str(&flat); }
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
                // For generic instantiations (e.g. `Box<Node*>`), the flat
                // name must include the mangled type arguments so that
                // `Box<Node*>` → `Box_Node_ptr` (matches the specialized
                // struct/vtable symbols). When there are no type args,
                // `name_flat` is a passthrough.
                let flat = if t.type_args.is_empty() {
                    name_flat(type_name)
                } else {
                    let args_str = t.type_args.iter()
                        .map(ast_type_to_c_str)
                        .collect::<Vec<_>>()
                        .join(", ");
                    name_flat(&format!("{}<{}>", type_name, args_str))
                };
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
        AstExprData::IvarRef { obj, ivar, cls, .. } => {
            let obj_str = render_callee_expr(obj);
            if let Some(ref iv) = ivar {
                if let Some(ref cls_name) = cls {
                    if obj_str == "self" {
                        let flat = name_flat(cls_name);
                        format!("((struct {} *)self)->{}", flat, iv)
                    } else {
                        format!("{}->{}", obj_str, iv)
                    }
                } else {
                    format!("{}->{}", obj_str, iv)
                }
            } else {
                obj_str
            }
        }
        AstExprData::VarRef { name, .. } => {
            if ae.kind == AstExprKind::Self_ || ae.kind == AstExprKind::Super {
                "self".to_string()
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
        return CgExpr { kind: CgExprKind::Ident, type_str: Some("NPObject *".into()), line, col, data: CgExprData::Ident("self".into()) };
    }
    if ae.kind == AstExprKind::Super {
        return CgExpr { kind: CgExprKind::Ident, type_str: Some("NPObject *".into()), line, col, data: CgExprData::Ident("super".into()) };
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
        AstExprData::IvarRef { ivar, obj, cls, .. } => {
            let field = ivar.clone().unwrap_or_default();
            let obj_cg = convert_expr(obj, &class_infos);
            let ivar_type_str = cls.as_ref().and_then(|cls_name| {
                let flat = name_flat(cls_name);
                class_infos.get(&flat).and_then(|info| {
                    info.ivar_names.iter().position(|n| n == &field).map(|idx| info.ivar_types[idx].clone())
                })
            });
            let type_str = ivar_type_str.or(type_str);
            if let CgExprData::Ident(ref name) = obj_cg.data {
                if name == "self" || name == "_self" {
                    if let Some(ref cls_name) = cls {
                        let flat = name_flat(cls_name);
                        // Inline cast: ((struct Cls *)self)->ivar avoids stale _self
                        // when self is reassigned (e.g. self = [super init]).
                        return CgExpr { kind: CgExprKind::Arrow, type_str, line, col,
                            data: CgExprData::Arrow {
                                obj: Box::new(CgExpr {
                                    kind: CgExprKind::Cast, type_str: None, line, col,
                                    data: CgExprData::Cast {
                                        target_type: format!("struct {} *", flat),
                                        expr: Box::new(CgExpr { kind: CgExprKind::Ident, type_str: None, line, col, data: CgExprData::Ident("self".into()) }),
                                    },
                                }),
                                field,
                            },
                        };
                    }
                }
            }
            CgExpr { kind: CgExprKind::Arrow, type_str, line, col, data: CgExprData::Arrow { obj: Box::new(obj_cg), field } }
        }
        AstExprData::MsgSend { receiver, selector, args, is_class_method, is_super, super_name, .. } => {
            // Special case: [receiver class] -> ((NPClass *)((__nupa_root *)receiver)->isa)
            // The "class" method is auto-generated on every meta vtable but NOT registered
            // in class_infos, so normal vtable dispatch can't find it. Emit the direct
            // ivar access which is semantically equivalent for all ObjC objects.
            if selector == "class" && !*is_super && args.is_empty() {
                let obj_cg = convert_expr(receiver, &class_infos);
                let isa_access = CgExpr {
                    kind: CgExprKind::Arrow, type_str: None, line, col,
                    data: CgExprData::Arrow {
                        obj: Box::new(CgExpr {
                            kind: CgExprKind::Cast, type_str: None, line, col,
                            data: CgExprData::Cast {
                                target_type: "__nupa_root *".to_string(),
                                expr: Box::new(obj_cg),
                            },
                        }),
                        field: "isa".to_string(),
                    },
                };
                return CgExpr {
                    kind: CgExprKind::Cast, type_str, line, col,
                    data: CgExprData::Cast {
                        target_type: "NPClass *".to_string(),
                        expr: Box::new(isa_access),
                    },
                };
            }
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
                // Try to resolve vtable class from the receiver's type first.
                let sel = sanitize_sel_name(selector);
                vtable_class = receiver.expr_type.as_ref()
                    .and_then(|t| t.class_ref.as_ref())
                    .and_then(|cr| {
                        let flat = name_flat(cr);
                        class_infos.get(&flat).and_then(|info| {
                            if info.method_names.iter().any(|n| n == &sel) {
                                Some(info.class_name.clone())
                            } else {
                                // Walk superclass chain to find the method
                                let mut cur = cr.clone();
                                loop {
                                    let cflat = name_flat(&cur);
                                    if let Some(ci) = class_infos.get(&cflat) {
                                        if ci.method_names.iter().any(|n| n == &sel) {
                                            return Some(ci.class_name.clone());
                                        }
                                        match ci.super_name.clone() {
                                            Some(sup) => cur = sup,
                                            None => break,
                                        }
                                    } else {
                                        break;
                                    }
                                }
                                None
                            }
                        })
                    });
                // If the receiver is a concrete class variable, use its type.
                if vtable_class.is_none() {
                    if let AstExprData::VarRef { name, .. } = &receiver.data {
                        let flat = name_flat(name);
                        if let Some(info) = class_infos.get(&flat) {
                            // Check if this class (or its superclass chain) has the method
                            let mut cur = info.class_name.clone();
                            loop {
                                let cflat = name_flat(&cur);
                                if let Some(ci) = class_infos.get(&cflat) {
                                    if let Some(idx) = ci.method_names.iter().position(|n| n == &sel) {
                                        vtable_class = Some(ci.class_name.clone());
                                        effective_is_class = ci.is_class_methods[idx];
                                        break;
                                    }
                                    match ci.super_name.clone() {
                                        Some(sup) => cur = sup,
                                        None => break,
                                    }
                                } else {
                                    break;
                                }
                            }
                        }
                    }
                }
                if vtable_class.is_none() {
                    vtable_class = None;
                    for (_, info) in class_infos.iter() {
                        if let Some(idx) = info.method_names.iter().position(|n| n == &sel) {
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
                    } else {
                        // Class method on expression receiver (e.g. [[self class] alloc]):
                        // receiver expression evaluates to an NPClass*, pass it directly
                        call_args.push(convert_expr(receiver, &class_infos));
                    }
                } else {
                    // Class method on runtime-determined receiver (e.g. [[self class] alloc]):
                    // receiver itself is the class pointer, pass it directly
                    call_args.push(convert_expr(receiver, &class_infos));
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

            let sel_const = sel_const_name(selector);

            CgExpr {
                kind: CgExprKind::Call, type_str, line, col,
                data: CgExprData::Call {
                    name: sanitize_sel_name(selector),
                    args: call_args,
                    vtable_class,
                    alt_vtable_classes,
                    is_class_method: effective_is_class,
                    is_super: *is_super,
                    sel_const_name: Some(sel_const), method_index: None,
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
            let mut auto_is_class = false;
            if callee.is_none() {
                // Detect direct calls to NPObject method functions like
                // `NPObject_release(obj)`, `NPObject_retain(obj)`, `NPObject_dealloc(obj)`.
                // Convert these to vtable dispatch so the normal vtable path handles
                // arg casting (e.g. (NPObject *)(child)) instead of a bare C call.
                if let Some(method) = call_name.strip_prefix("NPObject_") {
                    if !method.is_empty() && args.len() == 1 {
                        auto_sel = Some(sel_const_name(method));
                        // Determine if class method (alloc/new) or instance method.
                        auto_is_class = class_infos.get("NPObject")
                            .map(|info| {
                                info.method_names.iter()
                                    .zip(info.is_class_methods.iter())
                                    .any(|(nm, is_cm)| nm == method && *is_cm)
                            })
                            .unwrap_or(false);
                    }
                }
            }
            let mut cg_args: Vec<CgExpr> = args.iter().map(|a| convert_expr(a, &class_infos)).collect();
            if let Some(sel) = auto_sel {
                // Convert NPObject_* calls to vtable dispatch by setting vtable_class,
                // sel_const_name, and using the method name (without NPObject_ prefix).
                let method = call_name.strip_prefix("NPObject_").unwrap();
                CgExpr {
                    kind: CgExprKind::Call, type_str, line, col,
                    data: CgExprData::Call {
                        name: method.to_string(),
                        args: cg_args, // no SEL — emitted from sel_const_name
                        vtable_class: Some("NPObject".to_string()),
                        alt_vtable_classes: vec![],
                        is_class_method: auto_is_class,
                        is_super: false,
                        sel_const_name: Some(sel),
                        method_index: None,
                    },
                }
            } else {
                CgExpr {
                    kind: CgExprKind::Call, type_str, line, col,
                    data: CgExprData::Call {
                        name: call_name, args: cg_args,
                        vtable_class: None, alt_vtable_classes: vec![], is_class_method: false, is_super: false,
                        sel_const_name: None, method_index: None,
                    },
                }
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
                        let sel_const = sel_const_name(&setter_sel);
                        CgExpr {
                            kind: CgExprKind::Call, type_str, line, col,
                            data: CgExprData::Call {
                                name: setter_sel.replace(':', "_"),
                                args: vec![obj_cg, sum_cg],
                                vtable_class,
                                alt_vtable_classes: vec![],
                                is_class_method: false,
                                is_super: false,
                                sel_const_name: Some(sel_const), method_index: None,
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
            if let AstExprData::PropRef { obj, name, is_arrow, prop, cls, .. } = &object.data {
                // Only prepend `_` for ObjC property access (prop is Some).
                let field_name = if prop.is_some() { format!("_{}", name) } else { name.clone() };
                let obj_cg = convert_expr(obj, &class_infos);
                let key_cg = convert_expr(key, &class_infos);
                // Inline cast self to avoid stale _self when self is reassigned
                let arr_obj = if let CgExprData::Ident(ref name) = obj_cg.data {
                    if name == "self" {
                        if let Some(ref cls_name) = cls {
                            let flat = name_flat(cls_name);
                            Box::new(CgExpr {
                                kind: CgExprKind::Cast, type_str: None, line, col,
                                data: CgExprData::Cast {
                                    target_type: format!("struct {} *", flat),
                                    expr: Box::new(CgExpr { kind: CgExprKind::Ident, type_str: None, line, col, data: CgExprData::Ident("self".into()) }),
                                },
                            })
                        } else {
                            Box::new(obj_cg)
                        }
                    } else {
                        Box::new(obj_cg)
                    }
                } else {
                    Box::new(obj_cg)
                };
                // Respect `.`/`->` from source: non-ObjC struct field access emits `.`
                // (Member) rather than `->` (Arrow).
                let field_cg = if *is_arrow {
                    CgExpr { kind: CgExprKind::Arrow, type_str: None, line, col,
                             data: CgExprData::Arrow { obj: arr_obj, field: field_name.clone() } }
                } else {
                    CgExpr { kind: CgExprKind::Member, type_str: None, line, col,
                             data: CgExprData::Member { obj: arr_obj, field: field_name } }
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
            CgExpr { kind: CgExprKind::Ident, type_str: None, line, col, data: CgExprData::Ident(format!("{}.hash", sel_const_name(s))) }
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

                let sel_const = sel_const_name(&name);

                CgExpr {
                    kind: CgExprKind::Call, type_str, line, col,
                    data: CgExprData::Call {
                        name: sel, args: vec![recv_cg],
                        vtable_class, alt_vtable_classes: vec![], is_class_method: false, is_super: false,
                        sel_const_name: Some(sel_const), method_index: None,
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
                    CgDeclData::Variable { var_type, init, is_static, is_weak, is_block, next, .. } => {
                        let (decl_type, array_suffix) = if let Some(pos) = var_type.find('[') {
                            (var_type[..pos].trim().to_string(), Some(var_type[pos..].to_string()))
                        } else {
                            (var_type, None)
                        };
                        CgStmt {
                            kind: CgStmtKind::Decl, line, col,
                            data: CgStmtData::Decl { decl_type, name: decl_cg.name, init, array_suffix, is_static, is_weak, is_block, next },
                        }
                    }
                    _ => CgStmt { kind: CgStmtKind::Empty, line, col, data: CgStmtData::Return(None) },
                }
            } else {
                CgStmt { kind: CgStmtKind::Empty, line, col, data: CgStmtData::Return(None) }
            }
        }
        AstStmtData::Autoreleasepool(body) => {
            let cg_body = convert_stmt(body, &class_infos);
            let body_stmts = match cg_body.data {
                CgStmtData::Compound(ref stmts) => stmts.clone(),
                _ => vec![cg_body],
            };
            let mut stmts: Vec<CgStmt> = Vec::new();
            stmts.push(CgStmt {
                kind: CgStmtKind::Decl, line: 0, col: 0,
                data: CgStmtData::Decl {
                    decl_type: "nupa_autoreleasepool_t *".into(),
                    name: "__nupa_pool".into(),
                    init: Some(Box::new(CgExpr {
                        kind: CgExprKind::Ident, type_str: None, line: 0, col: 0,
                        data: CgExprData::Ident("nupa_autoreleasepool_push()".into()),
                    })),
                    array_suffix: None,
                    is_static: false,
                    is_weak: false,
                    is_block: false,
                    next: vec![],
                },
            });
            stmts.extend(body_stmts);
            stmts.push(CgStmt {
                kind: CgStmtKind::Expr, line: 0, col: 0,
                data: CgStmtData::Expr(CgExpr {
                    kind: CgExprKind::Ident, type_str: None, line: 0, col: 0,
                    data: CgExprData::Ident("nupa_autoreleasepool_pop(__nupa_pool)".into()),
                }),
            });
            CgStmt { kind: CgStmtKind::Compound, line, col, data: CgStmtData::Compound(stmts) }
        }
        AstStmtData::ForIn { var, collection, body } => {
            // Convert for-in to a simple for loop over the collection
            let var_cg = convert_expr(var, &class_infos);
            let col_cg = convert_expr(collection, &class_infos);
            let cg_body = convert_stmt(body, &class_infos);
            // For now, emit as compound with a comment
            CgStmt { kind: CgStmtKind::Compound, line, col, data: CgStmtData::Compound(Vec::new()) }
        }
        AstStmtData::Synchronized { body, .. } => {
            // Emit synchronized body as plain compound (no lock semantics yet)
            convert_stmt(body, class_infos)
        }
        AstStmtData::Try { try_block, catches, finally_block } => {
            // setjmp/longjmp exception handling with save/restore for nesting
            let try_cg = convert_stmt(try_block, class_infos);
            let finally_cg = finally_block.as_ref().map(|fb| convert_stmt(fb, class_infos));

            // Build the catch blocks: each Catch { param, body } becomes:
            //   { param_type param_name = __nupa_exception_value; body }
            // Wrap in "if (__nupa_state == 1) { __nupa_state = 2; <catches> }"
            let mut catch_body: Vec<CgStmt> = Vec::new();
            if !catches.is_empty() {
                let mut catch_stmts: Vec<CgStmt> = Vec::new();
                for c in catches.iter() {
                    if let AstStmtData::Catch { param, body } = &c.data {
                        let param_type = param.par_type.as_ref()
                            .map(|pt| cst_type_to_c_str(pt))
                            .unwrap_or_else(|| "id".into());
                        let param_name = param.name.clone().unwrap_or_else(|| "exc".into());
                        catch_stmts.push(CgStmt {
                            kind: CgStmtKind::Decl, line, col,
                            data: CgStmtData::Decl {
                                decl_type: param_type,
                                name: param_name,
                                init: Some(Box::new(CgExpr {
                                    kind: CgExprKind::Ident, type_str: None, line, col,
                                data: CgExprData::Ident("__nupa_exception_value".into()),
                            })),
                            array_suffix: None,
                            is_static: false,
                            is_weak: false,
                            is_block: false,
                            next: vec![],
                        },
                    });
                    catch_stmts.push(convert_stmt(&*body, class_infos));
                }
                }
                // __nupa_state = 2 (caught)
                catch_stmts.insert(0, CgStmt {
                    kind: CgStmtKind::Expr, line, col,
                    data: CgStmtData::Expr(CgExpr {
                        kind: CgExprKind::Assign, type_str: None, line, col,
                        data: CgExprData::Assign {
                            target: Box::new(CgExpr {
                                kind: CgExprKind::Ident, type_str: None, line, col,
                                data: CgExprData::Ident("__nupa_state".into()),
                            }),
                            value: Box::new(CgExpr {
                                kind: CgExprKind::Int, type_str: None, line, col,
                                data: CgExprData::Int(2),
                            }),
                        },
                    }),
                });
                // if (__nupa_state == 1) { ... }
                let state_cond = CgExpr {
                    kind: CgExprKind::Binary, type_str: None, line, col,
                    data: CgExprData::Binary {
                        op_str: "==".into(),
                        left: Box::new(CgExpr {
                            kind: CgExprKind::Ident, type_str: None, line, col,
                            data: CgExprData::Ident("__nupa_state".into()),
                        }),
                        right: Box::new(CgExpr {
                            kind: CgExprKind::Int, type_str: None, line, col,
                            data: CgExprData::Int(1),
                        }),
                    },
                };
                catch_body.push(CgStmt {
                    kind: CgStmtKind::If, line, col,
                    data: CgStmtData::If {
                        cond: Box::new(state_cond),
                        then: Box::new(CgStmt {
                            kind: CgStmtKind::Compound, line, col,
                            data: CgStmtData::Compound(catch_stmts),
                        }),
                        else_: None,
                    },
                });
            }

            // ── Build the full try/catch/finally pattern ──
            // {
            //   jmp_buf __nupa_saved;
            //   memcpy(__nupa_saved, __nupa_exception_buf, sizeof(jmp_buf));
            //   volatile int __nupa_state = 0;
            //   if (setjmp(__nupa_exception_buf) != 0) { __nupa_state = 1; }
            //   if (__nupa_state == 0) { <try_body> }
            //   <catch_body_if_state_1>
            //   memcpy(__nupa_exception_buf, __nupa_saved, sizeof(jmp_buf));
            //   <finally_block>
            //   if (__nupa_state == 1) { longjmp(__nupa_exception_buf, 1); }
            // }
            let mut try_stmts: Vec<CgStmt> = Vec::new();

            // jmp_buf __nupa_saved;
            try_stmts.push(CgStmt {
                kind: CgStmtKind::Decl, line, col,
                data: CgStmtData::Decl {
                    decl_type: "jmp_buf".into(),
                    name: "__nupa_saved".into(),
                    init: None,
                    array_suffix: None,
                    is_static: false,
                    is_weak: false,
                    is_block: false,
                    next: vec![],
                },
            });

            // memcpy(__nupa_saved, __nupa_exception_buf, sizeof(jmp_buf));
            try_stmts.push(CgStmt {
                kind: CgStmtKind::Expr, line, col,
                data: CgStmtData::Expr(CgExpr {
                    kind: CgExprKind::Call, type_str: None, line, col,
                    data: CgExprData::Call {
                        name: "memcpy".into(),
                        args: vec![
                            CgExpr {
                                kind: CgExprKind::Ident, type_str: None, line, col,
                                data: CgExprData::Ident("__nupa_saved".into()),
                            },
                            CgExpr {
                                kind: CgExprKind::Ident, type_str: None, line, col,
                                data: CgExprData::Ident("__nupa_exception_buf".into()),
                            },
                            CgExpr {
                                kind: CgExprKind::Sizeof, type_str: None, line, col,
                                data: CgExprData::Sizeof("jmp_buf".into()),
                            },
                        ],
                        vtable_class: None,
                        alt_vtable_classes: vec![],
                        is_class_method: false,
                        is_super: false,
                        sel_const_name: None, method_index: None,
                    },
                }),
            });

            // volatile int __nupa_state = 0;
            try_stmts.push(CgStmt {
                kind: CgStmtKind::Decl, line, col,
                data: CgStmtData::Decl {
                    decl_type: "volatile int".into(),
                    name: "__nupa_state".into(),
                    init: Some(Box::new(CgExpr {
                        kind: CgExprKind::Int, type_str: None, line, col,
                        data: CgExprData::Int(0),
                    })),
                    array_suffix: None,
                    is_static: false,
                    is_weak: false,
                    is_block: false,
                    next: vec![],
                },
            });

            // if (setjmp(__nupa_exception_buf) != 0) { __nupa_state = 1; }
            try_stmts.push(CgStmt {
                kind: CgStmtKind::If, line, col,
                data: CgStmtData::If {
                    cond: Box::new(CgExpr {
                        kind: CgExprKind::Binary, type_str: None, line, col,
                        data: CgExprData::Binary {
                            op_str: "!=".into(),
                            left: Box::new(CgExpr {
                                kind: CgExprKind::Call, type_str: None, line, col,
                                data: CgExprData::Call {
                                    name: "setjmp".into(),
                                    args: vec![CgExpr {
                                        kind: CgExprKind::Ident, type_str: None, line, col,
                                        data: CgExprData::Ident("__nupa_exception_buf".into()),
                                    }],
                                    vtable_class: None, alt_vtable_classes: vec![],
                                    is_class_method: false, is_super: false, sel_const_name: None, method_index: None,
                                },
                            }),
                            right: Box::new(CgExpr {
                                kind: CgExprKind::Int, type_str: None, line, col,
                                data: CgExprData::Int(0),
                            }),
                        },
                    }),
                    then: Box::new(CgStmt {
                        kind: CgStmtKind::Expr, line, col,
                        data: CgStmtData::Expr(CgExpr {
                            kind: CgExprKind::Assign, type_str: None, line, col,
                            data: CgExprData::Assign {
                                target: Box::new(CgExpr {
                                    kind: CgExprKind::Ident, type_str: None, line, col,
                                    data: CgExprData::Ident("__nupa_state".into()),
                                }),
                                value: Box::new(CgExpr {
                                    kind: CgExprKind::Int, type_str: None, line, col,
                                    data: CgExprData::Int(1),
                                }),
                            },
                        }),
                    }),
                    else_: None,
                },
            });

            // if (__nupa_state == 0) { <try_body> }
            try_stmts.push(CgStmt {
                kind: CgStmtKind::If, line, col,
                data: CgStmtData::If {
                    cond: Box::new(CgExpr {
                        kind: CgExprKind::Binary, type_str: None, line, col,
                        data: CgExprData::Binary {
                            op_str: "==".into(),
                            left: Box::new(CgExpr {
                                kind: CgExprKind::Ident, type_str: None, line, col,
                                data: CgExprData::Ident("__nupa_state".into()),
                            }),
                            right: Box::new(CgExpr {
                                kind: CgExprKind::Int, type_str: None, line, col,
                                data: CgExprData::Int(0),
                            }),
                        },
                    }),
                    then: Box::new(try_cg),
                    else_: None,
                },
            });

            // catch blocks (if any)
            try_stmts.extend(catch_body);

            // memcpy(__nupa_exception_buf, __nupa_saved, sizeof(jmp_buf));
            try_stmts.push(CgStmt {
                kind: CgStmtKind::Expr, line, col,
                data: CgStmtData::Expr(CgExpr {
                    kind: CgExprKind::Call, type_str: None, line, col,
                    data: CgExprData::Call {
                        name: "memcpy".into(),
                        args: vec![
                            CgExpr { kind: CgExprKind::Ident, type_str: None, line, col, data: CgExprData::Ident("__nupa_exception_buf".into()) },
                            CgExpr { kind: CgExprKind::Ident, type_str: None, line, col, data: CgExprData::Ident("__nupa_saved".into()) },
                            CgExpr { kind: CgExprKind::Sizeof, type_str: None, line, col, data: CgExprData::Sizeof("jmp_buf".into()) },
                        ],
                        vtable_class: None, alt_vtable_classes: vec![],
                        is_class_method: false, is_super: false, sel_const_name: None, method_index: None,
                    },
                }),
            });

            // finally block
            if let Some(f) = finally_cg {
                try_stmts.push(f);
            }

            // if (__nupa_state == 1) { longjmp(__nupa_exception_buf, 1); }
            try_stmts.push(CgStmt {
                kind: CgStmtKind::If, line, col,
                data: CgStmtData::If {
                    cond: Box::new(CgExpr {
                        kind: CgExprKind::Binary, type_str: None, line, col,
                        data: CgExprData::Binary {
                            op_str: "==".into(),
                            left: Box::new(CgExpr {
                                kind: CgExprKind::Ident, type_str: None, line, col,
                                data: CgExprData::Ident("__nupa_state".into()),
                            }),
                            right: Box::new(CgExpr {
                                kind: CgExprKind::Int, type_str: None, line, col,
                                data: CgExprData::Int(1),
                            }),
                        },
                    }),
                    then: Box::new(CgStmt {
                        kind: CgStmtKind::Expr, line, col,
                        data: CgStmtData::Expr(CgExpr {
                            kind: CgExprKind::Call, type_str: None, line, col,
                            data: CgExprData::Call {
                                name: "longjmp".into(),
                                args: vec![
                                    CgExpr { kind: CgExprKind::Ident, type_str: None, line, col, data: CgExprData::Ident("__nupa_exception_buf".into()) },
                                    CgExpr { kind: CgExprKind::Int, type_str: None, line, col, data: CgExprData::Int(1) },
                                ],
                                vtable_class: None, alt_vtable_classes: vec![],
                                is_class_method: false, is_super: false, sel_const_name: None, method_index: None,
                            },
                        }),
                    }),
                    else_: None,
                },
            });

            CgStmt { kind: CgStmtKind::Compound, line, col, data: CgStmtData::Compound(try_stmts) }
        }
        AstStmtData::Catch { .. } | AstStmtData::Finally(_) => {
            CgStmt { kind: CgStmtKind::Compound, line, col, data: CgStmtData::Compound(Vec::new()) }
        }
        AstStmtData::Throw(expr) => {
            let mut stmts: Vec<CgStmt> = Vec::new();
            if let Some(e) = expr {
                // __nupa_exception_value = (expr);
                stmts.push(CgStmt {
                    kind: CgStmtKind::Expr, line, col,
                    data: CgStmtData::Expr(CgExpr {
                        kind: CgExprKind::Assign, type_str: None, line, col,
                        data: CgExprData::Assign {
                            target: Box::new(CgExpr {
                                kind: CgExprKind::Ident, type_str: None, line, col,
                                data: CgExprData::Ident("__nupa_exception_value".into()),
                            }),
                            value: Box::new(convert_expr(e, class_infos)),
                        },
                    }),
                });
            }
            // longjmp(__nupa_exception_buf, 1);
            stmts.push(CgStmt {
                kind: CgStmtKind::Expr, line, col,
                data: CgStmtData::Expr(CgExpr {
                    kind: CgExprKind::Call, type_str: None, line, col,
                    data: CgExprData::Call {
                        name: "longjmp".into(),
                        args: vec![
                            CgExpr {
                                kind: CgExprKind::Ident, type_str: None, line, col,
                                data: CgExprData::Ident("__nupa_exception_buf".into()),
                            },
                            CgExpr {
                                kind: CgExprKind::Int, type_str: None, line, col,
                                data: CgExprData::Int(1),
                            },
                        ],
                        vtable_class: None,
                        alt_vtable_classes: vec![],
                        is_class_method: false,
                        is_super: false,
                        sel_const_name: None, method_index: None,
                    },
                }),
            });
            CgStmt { kind: CgStmtKind::Compound, line, col, data: CgStmtData::Compound(stmts) }
        }
        _ => CgStmt { kind: CgStmtKind::Expr, line, col, data: CgStmtData::Expr(CgExpr { kind: CgExprKind::Call, type_str: None, line, col, data: CgExprData::Call { name: "/* stub */".into(), args: Vec::new(), vtable_class: None, alt_vtable_classes: vec![], is_class_method: false, is_super: false, sel_const_name: None, method_index: None } }) },
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
                        sel_const_name: None, method_index: None,
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
            let var_type = if is_weak && !var_type.contains("__block") {
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
                data: CgDeclData::Variable { var_type, init, is_static, is_const, is_weak, is_block: is_block_qual, next: next_decls },
            });
        }
        AstDeclKind::Typedef => {
            let (mut alias_type_str, struct_fields, has_block_name) = match &ad.data {
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
            // For block typedefs, replace short block name with namespace-prefixed flat name
            // so the canonical typedef uses the fully qualified name directly.
            // No short-name alias is emitted — all references must use the flat name.
            if has_block_name {
                let short_block_name = match &ad.data {
                    AstDeclData::Typedef { aliased_type, .. } => {
                        aliased_type.as_ref().and_then(|at| at.block_name.clone())
                    }
                    _ => None,
                };
                if let Some(ref sn) = short_block_name {
                    if let Some(pos) = alias_type_str.find(sn.as_str()) {
                        alias_type_str.replace_range(pos..pos + sn.len(), &flat_alias);
                    }
                    if let Ok(mut guard) = BLOCK_TYPEDEF_NAMES.get_or_init(|| Mutex::new(HashMap::new())).lock() {
                        guard.insert(sn.clone(), flat_alias.clone());
                    }
                }
            }
            let alias = if has_block_name { String::new() } else { flat_alias.clone() };
            result.push(CgDecl {
                kind: CgDeclKind::Typedef, name: ad.name.clone().unwrap_or_default(),
                data: CgDeclData::Typedef { alias, type_str: alias_type_str.clone(), struct_fields },
            });
        }
        AstDeclKind::Struct => {
            let fields = match &ad.data {
                AstDeclData::Aggregate { fields } => fields,
                _ => &Vec::new(),
            };
            if fields.is_empty() {
                result.push(CgDecl { kind: CgDeclKind::Variable, name, data: CgDeclData::Variable { var_type: "void".into(), init: None, is_static: false, is_const: false, is_weak: false, is_block: false, next: vec![] } });
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
            result.push(CgDecl { kind: CgDeclKind::Variable, name, data: CgDeclData::Variable { var_type: "void".into(), init: None, is_static: false, is_const: false, is_weak: false, is_block: false, next: vec![] } });
        }
        AstDeclKind::Ivar | AstDeclKind::Method | AstDeclKind::Property | AstDeclKind::Union | AstDeclKind::Namespace => {
            result.push(CgDecl { kind: CgDeclKind::Variable, name, data: CgDeclData::Variable { var_type: "int".into(), init: None, is_static: false, is_const: false, is_weak: false, is_block: false, next: vec![] } });
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

/// Extract the canonical signature key from a block type string.
/// E.g. `void (^CollisionEventBlock)(int, float *)` → `void(int, float *)`.
/// Used for block typedef dedup: two block types with the same return/param
/// signature shouldn't emit duplicate full typedefs.
fn block_type_signature_key(s: &str) -> String {
    if let Some(hat_pos) = s.find("(^") {
        let ret_part = s[..hat_pos].trim();
        let after = &s[hat_pos + 2..];
        if let Some(close_paren) = after.find(")(") {
            let params_block = &after[close_paren + 2..];
            if let Some(end) = params_block.rfind(')') {
                return format!("{}({})", ret_part, &params_block[..end]);
            }
        }
    }
    s.to_string()
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

/// Rewrite a cloned CgStmt tree for a monomorphized generic class.
///
/// When `Box<T>` is specialized to `Box<Node*>`, the cloned method body still
/// refers to the generic template: `struct Box * _self = (struct Box *)self;`
/// and `T item` (rendered as `NPObject * item`). This walks the CgStmt/CgExpr
/// tree and replaces:
///   * `struct {base_flat} *` → `struct {mangled_flat} *`   (the `_self` cast)
///   * `{t_render}` → `{concrete_str}`                       (T → concrete)
///
/// `t_render` is the C string used for `TypePrim::Param` (typically `NPObject *`);
/// `concrete_str` is the rendered concrete type (e.g. `Node *`).
fn substitute_cg_stmt(
    s: &mut CgStmt,
    base_flat: &str,
    mangled_flat: &str,
    t_render: &str,
    concrete_str: &str,
) {
    match &mut s.data {
        CgStmtData::Expr(e) => substitute_cg_expr(e, base_flat, mangled_flat, t_render, concrete_str),
        CgStmtData::Compound(stmts) => {
            for st in stmts.iter_mut() {
                substitute_cg_stmt(st, base_flat, mangled_flat, t_render, concrete_str);
            }
        }
        CgStmtData::If { cond, then, else_ } => {
            substitute_cg_expr(cond, base_flat, mangled_flat, t_render, concrete_str);
            substitute_cg_stmt(then, base_flat, mangled_flat, t_render, concrete_str);
            if let Some(eb) = else_ {
                substitute_cg_stmt(eb, base_flat, mangled_flat, t_render, concrete_str);
            }
        }
        CgStmtData::While { cond, body } => {
            substitute_cg_expr(cond, base_flat, mangled_flat, t_render, concrete_str);
            substitute_cg_stmt(body, base_flat, mangled_flat, t_render, concrete_str);
        }
        CgStmtData::Do { body, cond } => {
            substitute_cg_stmt(body, base_flat, mangled_flat, t_render, concrete_str);
            substitute_cg_expr(cond, base_flat, mangled_flat, t_render, concrete_str);
        }
        CgStmtData::For { init, cond, incr, body } => {
            if let Some(i) = init {
                substitute_cg_stmt(i, base_flat, mangled_flat, t_render, concrete_str);
            }
            if let Some(c) = cond {
                substitute_cg_expr(c, base_flat, mangled_flat, t_render, concrete_str);
            }
            if let Some(u) = incr {
                substitute_cg_expr(u, base_flat, mangled_flat, t_render, concrete_str);
            }
            substitute_cg_stmt(body, base_flat, mangled_flat, t_render, concrete_str);
        }
        CgStmtData::Return(v) => {
            if let Some(e) = v {
                substitute_cg_expr(e, base_flat, mangled_flat, t_render, concrete_str);
            }
        }
        CgStmtData::Switch { expr, body } => {
            substitute_cg_expr(expr, base_flat, mangled_flat, t_render, concrete_str);
            substitute_cg_stmt(body, base_flat, mangled_flat, t_render, concrete_str);
        }
        CgStmtData::Case { value, body } => {
            substitute_cg_expr(value, base_flat, mangled_flat, t_render, concrete_str);
            substitute_cg_stmt(body, base_flat, mangled_flat, t_render, concrete_str);
        }
        CgStmtData::Default(b) => {
            substitute_cg_stmt(b, base_flat, mangled_flat, t_render, concrete_str);
        }
        CgStmtData::Decl {
            decl_type,
            init,
            next,
            ..
        } => {
            rewrite_type_str(decl_type, base_flat, mangled_flat, t_render, concrete_str);
            if let Some(init_expr) = init {
                substitute_cg_expr(init_expr, base_flat, mangled_flat, t_render, concrete_str);
            }
            for (_, init_opt) in next.iter_mut() {
                if let Some(e) = init_opt {
                    substitute_cg_expr(e, base_flat, mangled_flat, t_render, concrete_str);
                }
            }
        }
        CgStmtData::ForIn { collection, body, .. } => {
            substitute_cg_expr(collection, base_flat, mangled_flat, t_render, concrete_str);
            substitute_cg_stmt(body, base_flat, mangled_flat, t_render, concrete_str);
        }
        _ => {}
    }
}

fn substitute_cg_expr(
    e: &mut CgExpr,
    base_flat: &str,
    mangled_flat: &str,
    t_render: &str,
    concrete_str: &str,
) {
    match &mut e.data {
        CgExprData::Cast { target_type, expr } => {
            rewrite_type_str(target_type, base_flat, mangled_flat, t_render, concrete_str);
            substitute_cg_expr(expr, base_flat, mangled_flat, t_render, concrete_str);
        }
        CgExprData::Unary { operand, .. } => {
            substitute_cg_expr(operand, base_flat, mangled_flat, t_render, concrete_str);
        }
        CgExprData::Binary { left, right, .. } => {
            substitute_cg_expr(left, base_flat, mangled_flat, t_render, concrete_str);
            substitute_cg_expr(right, base_flat, mangled_flat, t_render, concrete_str);
        }
        CgExprData::Assign { target, value } => {
            substitute_cg_expr(target, base_flat, mangled_flat, t_render, concrete_str);
            substitute_cg_expr(value, base_flat, mangled_flat, t_render, concrete_str);
        }
        CgExprData::Call {
            args,
            vtable_class,
            alt_vtable_classes,
            ..
        } => {
            for a in args.iter_mut() {
                substitute_cg_expr(a, base_flat, mangled_flat, t_render, concrete_str);
            }
            // Specialized calls should dispatch through the specialized vtable.
            if let Some(vc) = vtable_class {
                if name_flat(vc) == base_flat {
                    *vc = mangled_flat.to_string();
                }
            }
            for alt in alt_vtable_classes.iter_mut() {
                if name_flat(alt) == base_flat {
                    *alt = mangled_flat.to_string();
                }
            }
        }
        CgExprData::Comma(exprs) => {
            for x in exprs.iter_mut() {
                substitute_cg_expr(x, base_flat, mangled_flat, t_render, concrete_str);
            }
        }
        CgExprData::Member { obj, .. } => {
            substitute_cg_expr(obj, base_flat, mangled_flat, t_render, concrete_str);
        }
        CgExprData::Arrow { obj, .. } => {
            substitute_cg_expr(obj, base_flat, mangled_flat, t_render, concrete_str);
        }
        CgExprData::Index { arr, index } => {
            substitute_cg_expr(arr, base_flat, mangled_flat, t_render, concrete_str);
            substitute_cg_expr(index, base_flat, mangled_flat, t_render, concrete_str);
        }
        CgExprData::Ternary { cond, then, else_ } => {
            substitute_cg_expr(cond, base_flat, mangled_flat, t_render, concrete_str);
            substitute_cg_expr(then, base_flat, mangled_flat, t_render, concrete_str);
            substitute_cg_expr(else_, base_flat, mangled_flat, t_render, concrete_str);
        }
        CgExprData::InitList(exprs) => {
            for x in exprs.iter_mut() {
                substitute_cg_expr(x, base_flat, mangled_flat, t_render, concrete_str);
            }
        }
        _ => {}
    }
}

/// Rewrite a C type string in place for a monomorphized generic class.
///
/// Replaces `struct {base_flat} *` → `struct {mangled_flat} *` and
/// `{t_render}` → `{concrete_str}`. The struct replacement only fires when
/// the base flat name appears as a `struct Name *` pattern, so unrelated
/// types containing the base name as a substring are left alone.
fn rewrite_type_str(
    s: &mut String,
    base_flat: &str,
    mangled_flat: &str,
    t_render: &str,
    concrete_str: &str,
) {
    if s.is_empty() {
        return;
    }
    // T → concrete (T renders as `NPObject *` in the template)
    if !t_render.is_empty() && s.contains(t_render) {
        *s = s.replace(t_render, concrete_str);
    }
    // `struct {base_flat} *` → `struct {mangled_flat} *`
    let needle = format!("struct {} *", base_flat);
    if s.contains(&needle) {
        let repl = format!("struct {} *", mangled_flat);
        *s = s.replace(&needle, &repl);
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

    fn collect_sel_expr(e: &AstExpr, sels: &mut Vec<String>) {
        if let AstExprData::Selector(s) = &e.data {
            add_sel(sels, s);
        }
        match &e.data {
            AstExprData::FuncCall { args, .. } => {
                for a in args { collect_sel_expr(a, sels); }
            }
            AstExprData::MsgSend { receiver, args, .. } => {
                collect_sel_expr(receiver, sels);
                for a in args { collect_sel_expr(a, sels); }
            }
            AstExprData::IvarRef { obj, .. } => collect_sel_expr(obj, sels),
            AstExprData::PropRef { obj, .. } => collect_sel_expr(obj, sels),
            AstExprData::Unary { operand, .. } => collect_sel_expr(operand, sels),
            AstExprData::Binary { left, right, .. } => { collect_sel_expr(left, sels); collect_sel_expr(right, sels); }
            AstExprData::Assign { target, value, .. } => { collect_sel_expr(target, sels); collect_sel_expr(value, sels); }
            AstExprData::Cast { expr, .. } => collect_sel_expr(expr, sels),
            AstExprData::Ternary { cond, then, else_, .. } => { collect_sel_expr(cond, sels); collect_sel_expr(then, sels); collect_sel_expr(else_, sels); }
            AstExprData::Subscript { object, key } => { collect_sel_expr(object, sels); collect_sel_expr(key, sels); }
            AstExprData::Comma(exprs) => { for ex in exprs { collect_sel_expr(ex, sels); } }
            AstExprData::ArrayLit(elements) => { for el in elements { collect_sel_expr(el, sels); } }
            _ => {}
        }
    }

    fn collect_sel_stmt(s: &AstStmt, sels: &mut Vec<String>) {
        match &s.data {
            AstStmtData::Expr(e) => collect_sel_expr(e, sels),
            AstStmtData::Decl(d) => {
                if let AstDeclData::Variable { init, .. } = &d.data {
                    if let Some(i) = init { collect_sel_expr(i, sels); }
                }
            }
            AstStmtData::If { cond, then, else_, .. } => {
                collect_sel_expr(cond, sels);
                collect_sel_stmt(then, sels);
                if let Some(el) = else_ { collect_sel_stmt(el, sels); }
            }
            AstStmtData::While { cond, body, .. } | AstStmtData::Do { body, cond } => {
                collect_sel_expr(cond, sels);
                collect_sel_stmt(body, sels);
            }
            AstStmtData::For { init, cond, incr, body, .. } => {
                if let Some(i) = init { collect_sel_stmt(i, sels); }
                if let Some(c) = cond { collect_sel_expr(c, sels); }
                if let Some(u) = incr { collect_sel_expr(u, sels); }
                collect_sel_stmt(body, sels);
            }
            AstStmtData::ForIn { collection, body, .. } => { collect_sel_expr(collection, sels); collect_sel_stmt(body, sels); }
            AstStmtData::Switch { expr, body } => {
                collect_sel_expr(expr, sels);
                collect_sel_stmt(body, sels);
            }
            AstStmtData::Case { value, body } => { collect_sel_expr(value, sels); collect_sel_stmt(body, sels); }
            AstStmtData::Default(body) => { collect_sel_stmt(body, sels); }
            AstStmtData::Compound(stmts) => { for st in stmts { collect_sel_stmt(st, sels); } }
            AstStmtData::Autoreleasepool(body) => { collect_sel_stmt(body, sels); }
            AstStmtData::Try { try_block, catches, finally_block } => {
                collect_sel_stmt(try_block, sels);
                for c in catches { collect_sel_stmt(c, sels); }
                if let Some(f) = finally_block { collect_sel_stmt(f, sels); }
            }
            AstStmtData::Throw(e) => { if let Some(ex) = e { collect_sel_expr(ex, sels); } }
            AstStmtData::Return(e) => { if let Some(ex) = e { collect_sel_expr(ex, sels); } }
            AstStmtData::Synchronized { lock, body } => { collect_sel_expr(lock, sels); collect_sel_stmt(body, sels); }
            _ => {}
        }
    }

    // Pre‑pass: collect block typedef short→flat name mappings so that ivar type
    // resolution (which happens before Typedef conversion) can use flat names.
    {
        let mut flat_decls: Vec<&AstDecl> = Vec::new();
        for d in &ast.decls { flatten_namespace_decls(d, &mut flat_decls); }
        for d in flat_decls {
            if d.kind == AstDeclKind::Typedef {
                if let AstDeclData::Typedef { aliased_type, .. } = &d.data {
                    if let Some(ref at) = aliased_type {
                        if let Some(ref bn) = at.block_name {
                            let flat = name_flat(d.name.as_deref().unwrap_or(""));
                            if let Ok(mut guard) = BLOCK_TYPEDEF_NAMES.get_or_init(|| Mutex::new(HashMap::new())).lock() {
                                guard.insert(bn.clone(), flat);
                            }
                        }
                    }
                }
            }
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
                    let mut it = ivar_type.as_ref().map(|t| ast_type_to_c_str(t)).unwrap_or_else(|| "int".into());
                    let in_ = iv.name.clone().unwrap_or_default();
                    // Resolve block typedef short names to namespace-prefixed flat names
                    if let Ok(guard) = BLOCK_TYPEDEF_NAMES.get_or_init(|| Mutex::new(HashMap::new())).lock() {
                        if let Some(flat) = guard.get(&it) {
                            it = flat.clone();
                        }
                    }
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
                                collect_sel_stmt(b, &mut selectors);
                                let cg_body = convert_stmt(b, &class_infos);
                                if !is_class {
                                    // No _self declaration — ivar access uses inline cast
                                    // ((struct Cls *)self)->ivar so self reassignment
                                    // (e.g. self = [super init]) never becomes stale.
                                    cg_body
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
                                                    vtable_class: None, alt_vtable_classes: vec![], is_class_method: false, is_super: false, sel_const_name: None, method_index: None,
                                                },
                                            })},
                                            CgStmt { kind: CgStmtKind::Expr, line: 0, col: 0, data: CgStmtData::Expr(assign_expr.clone()) },
                                            CgStmt { kind: CgStmtKind::Expr, line: 0, col: 0, data: CgStmtData::Expr(CgExpr {
                                                kind: CgExprKind::Call, type_str: None, line: 0, col: 0,
                                                data: CgExprData::Call {
                                                    name: "nupa_weak_register".into(),
                                                    args: vec![cast_addr.clone(), cast_value],
                                                    vtable_class: None, alt_vtable_classes: vec![], is_class_method: false, is_super: false, sel_const_name: None, method_index: None,
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
                if let Some(b) = body {
                    walk_stmt_for_inst(b, &mut generic_instantiations, &collect_instantiations_expr);
                    collect_sel_stmt(b, &mut selectors);
                }
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
            // Rewrite cloned method bodies so the `_self` cast and any T-derived
            // type strings refer to the specialized class, not the generic template.
            // e.g. `struct Box * _self = (struct Box *)self;` → `struct Box_Node_ptr * ...`
            for body_cell in sub_info.method_bodies.iter_mut() {
                if let Some(b) = body_cell.as_mut() {
                    substitute_cg_stmt(b, &base_flat, &mangled_flat, "NPObject *", &concrete_str);
                }
            }
            for (i, mname) in sub_info.method_names.iter().enumerate() {
                let fn_name = format!("{}_{}", mangled_flat, mname);
                let ret_type = sub_info.method_return_types.get(i).cloned().unwrap_or_else(|| "NPObject *".into());
                let params = sub_info.method_params_list.get(i).cloned().unwrap_or_default();
                let base_fn_name = format!("{}_{}", base_flat, mname);
                let body = sub_info.method_bodies.get(i)
                    .and_then(|b| b.as_ref().map(|b| Box::new(b.as_ref().clone())))
                    .or_else(|| {
                        decls.iter().find(|d| d.name == base_fn_name).and_then(|d| {
                            if let CgDeclData::Function { ref body, .. } = d.data {
                                let mut cloned = body.clone();
                                if let Some(ref mut c) = cloned {
                                    substitute_cg_stmt(c, &base_flat, &mangled_flat, "NPObject *", &concrete_str);
                                }
                                cloned
                            } else { None }
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
            AstDeclData::Variable { var_type, init, .. } => {
                if let Some(t) = var_type {
                    if !t.type_args.is_empty() {
                        out.push((t.name.clone().unwrap_or_default(), t.type_args.clone()));
                    }
                }
                if let Some(i) = init { walk_expr_for_inst(i, out, f); }
            }
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

    // Collect unique instance method names across all classes, assign global indices.
    let mut global_instance_method_names: Vec<String> = Vec::new();
    for cm in &classes {
        for (j, mname) in cm.method_names.iter().enumerate() {
            if !cm.is_class_methods[j] && !global_instance_method_names.contains(mname) {
                global_instance_method_names.push(mname.clone());
            }
        }
    }
    global_instance_method_names.sort();

    let mut method_meta: HashMap<String, (usize, String)> = HashMap::new();
    for (idx, mname) in global_instance_method_names.iter().enumerate() {
        // Find the first class that has this method to get its signature.
        for cm in &classes {
            if let Some(pos) = cm.method_names.iter().position(|n| n == mname) {
                if !cm.is_class_methods[pos] {
                    let rt = cm.method_return_types.get(pos).cloned().unwrap_or_else(|| "NPObject *".into());
                    let params = cm.method_params_list.get(pos)
                        .map(|p| p.iter().map(|(pt, _)| pt.clone()).collect::<Vec<_>>().join(", "))
                        .unwrap_or_else(|| "NPObject *, SEL".into());
                    let ptr_type = format!("{} (*)({})", rt, params);
                    method_meta.insert(mname.clone(), (idx, ptr_type));
                    break;
                }
            }
        }
    }

    // Populate vtable_indices for each class.
    for cm in &mut classes {
        cm.vtable_indices = Vec::new();
        for (j, mname) in cm.method_names.iter().enumerate() {
            if !cm.is_class_methods[j] {
                if let Some((idx, _)) = method_meta.get(mname) {
                    cm.vtable_indices.push(*idx as i32);
                } else {
                    cm.vtable_indices.push(-1);
                }
            } else {
                cm.vtable_indices.push(-1);
            }
        }
    }

    METHOD_METADATA.set(method_meta).unwrap();

    let mut unit = CgUnit { decls, filename: ast.filename.clone(), c_headers: Vec::new(), selectors, classes, global_instance_method_names };
    rewrite_block_var_refs(&mut unit);
    unit
}

fn rewrite_block_var_refs(unit: &mut CgUnit) {
    let mut block_vars: std::collections::HashSet<String> = unit.decls.iter()
        .filter_map(|d| {
            if let CgDeclData::Variable { is_block, .. } = &d.data {
                if *is_block { Some(d.name.clone()) } else { None }
            } else { None }
        })
        .collect();
    // Also collect __block variables from function body declarations
    fn collect_block_vars_from_stmt(stmt: &CgStmt, bv: &mut std::collections::HashSet<String>) {
        match &stmt.data {
            CgStmtData::Decl { is_block, name, .. } => {
                if *is_block { bv.insert(name.clone()); }
            }
            CgStmtData::Compound(stmts) => { for s in stmts { collect_block_vars_from_stmt(s, bv); } }
            CgStmtData::If { then, else_, .. } => {
                collect_block_vars_from_stmt(then, bv);
                if let Some(el) = else_ { collect_block_vars_from_stmt(el, bv); }
            }
            CgStmtData::Switch { body, .. } => collect_block_vars_from_stmt(body, bv),
            CgStmtData::Case { body, .. } => collect_block_vars_from_stmt(body, bv),
            CgStmtData::Default(body) => collect_block_vars_from_stmt(body, bv),
            CgStmtData::While { body, .. } => collect_block_vars_from_stmt(body, bv),
            CgStmtData::Do { body, .. } => collect_block_vars_from_stmt(body, bv),
            CgStmtData::For { init, body, .. } => {
                if let Some(i) = init { collect_block_vars_from_stmt(i, bv); }
                collect_block_vars_from_stmt(body, bv);
            }
            CgStmtData::ForIn { body, .. } => collect_block_vars_from_stmt(body, bv),
            _ => {}
        }
    }
    for decl in &unit.decls {
        if let CgDeclData::Function { body, .. } = &decl.data {
            if let Some(ref b) = body {
                collect_block_vars_from_stmt(b, &mut block_vars);
            }
        }
    }
    if block_vars.is_empty() { return; }
    fn rewrite_expr(e: &mut CgExpr, bv: &std::collections::HashSet<String>) {
        match &mut e.data {
            CgExprData::Ident(name) => {
                if bv.contains(name.as_str()) {
                    let name_clone = name.clone();
                    *e = CgExpr {
                        kind: CgExprKind::Arrow,
                        type_str: None,
                        line: e.line, col: e.col,
                        data: CgExprData::Arrow {
                            obj: Box::new(CgExpr {
                                kind: CgExprKind::Member,
                                type_str: None,
                                line: e.line, col: e.col,
                                data: CgExprData::Member {
                                    obj: Box::new(CgExpr {
                                        kind: CgExprKind::Ident,
                                        type_str: None,
                                        line: e.line, col: e.col,
                                        data: CgExprData::Ident(name_clone),
                                    }),
                                    field: "__forwarding".into(),
                                },
                            }),
                            field: "__value".into(),
                        },
                    };
                }
            }
            CgExprData::Unary { operand, .. } => rewrite_expr(operand, bv),
            CgExprData::Binary { left, right, .. } => { rewrite_expr(left, bv); rewrite_expr(right, bv); }
            CgExprData::Assign { target, value, .. } => { rewrite_expr(target, bv); rewrite_expr(value, bv); }
            CgExprData::Cast { expr, .. } => rewrite_expr(expr, bv),
            CgExprData::Call { args, .. } => { for a in args { rewrite_expr(a, bv); } }
            CgExprData::Comma(exprs) => { for e in exprs { rewrite_expr(e, bv); } }
            CgExprData::Member { obj, .. } => rewrite_expr(obj, bv),
            CgExprData::Arrow { obj, .. } => rewrite_expr(obj, bv),
            CgExprData::Index { arr, index } => { rewrite_expr(arr, bv); rewrite_expr(index, bv); }
            CgExprData::Ternary { cond, then, else_ } => { rewrite_expr(cond, bv); rewrite_expr(then, bv); rewrite_expr(else_, bv); }
            CgExprData::InitList(elements) => { for e in elements { rewrite_expr(e, bv); } }
            CgExprData::BlockLit(data) => {
                if let Some(ref mut body) = data.body {
                    rewrite_stmt(body, bv);
                }
            }
            _ => {}
        }
    }
    fn rewrite_stmt(s: &mut CgStmt, bv: &std::collections::HashSet<String>) {
        match &mut s.data {
            CgStmtData::Expr(e) => rewrite_expr(e, bv),
            CgStmtData::Compound(stmts) => { for st in stmts { rewrite_stmt(st, bv); } }
            CgStmtData::If { cond, then, else_ } => { rewrite_expr(cond, bv); rewrite_stmt(then, bv); if let Some(el) = else_ { rewrite_stmt(el, bv); } }
            CgStmtData::Switch { expr, body } => { rewrite_expr(expr, bv); rewrite_stmt(body, bv); }
            CgStmtData::Case { value, body } => { rewrite_expr(value, bv); rewrite_stmt(body, bv); }
            CgStmtData::Default(body) => rewrite_stmt(body, bv),
            CgStmtData::While { cond, body } => { rewrite_expr(cond, bv); rewrite_stmt(body, bv); }
            CgStmtData::Do { body, cond } => { rewrite_stmt(body, bv); rewrite_expr(cond, bv); }
            CgStmtData::For { init, cond, incr, body } => {
                if let Some(i) = init { rewrite_stmt(i, bv); }
                if let Some(c) = cond { rewrite_expr(c, bv); }
                if let Some(u) = incr { rewrite_expr(u, bv); }
                rewrite_stmt(body, bv);
            }
            CgStmtData::ForIn { collection, body, .. } => { rewrite_expr(collection, bv); rewrite_stmt(body, bv); }
            CgStmtData::Return(v) => { if let Some(e) = v { rewrite_expr(e, bv); } }
            CgStmtData::Decl { init, next, .. } => {
                if let Some(i) = init { rewrite_expr(i, bv); }
                for (_, i) in next { if let Some(ex) = i { rewrite_expr(ex, bv); } }
            }
            _ => {}
        }
    }
    for decl in &mut unit.decls {
        match &mut decl.data {
            CgDeclData::Function { body, .. } => {
                if let Some(ref mut b) = body { rewrite_stmt(b, &block_vars); }
            }
            CgDeclData::Variable { init, next, .. } => {
                if let Some(ref mut i) = init { rewrite_expr(i, &block_vars); }
                for (_, i) in next { if let Some(ref mut ex) = i { rewrite_expr(ex, &block_vars); } }
            }
            _ => {}
        }
    }
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
            // Omit parens for == / != to avoid -Wparentheses-equality from
            // double-wrapping when used inside if()/while().  All other binary
            // ops need parens to guarantee correct nesting inside adjacent ops.
            if op_str == "==" || op_str == "!=" {
                emit_expr(left, out);
                out.push(' ');
                out.push_str(op_str);
                out.push(' ');
                emit_expr(right, out);
            } else {
                out.push('(');
                emit_expr(left, out);
                out.push(' ');
                out.push_str(op_str);
                out.push(' ');
                emit_expr(right, out);
                out.push(')');
            }
        }
        CgExprData::Assign { target, value } => {
            emit_expr(target, out);
            out.push_str(" = ");
            // Cast the value when target is a subclass pointer and value is a vtable call
            if let Some(ttype) = &target.type_str {
                if ttype.ends_with(" *") && ttype != "NPObject *" && matches!(value.data, CgExprData::Call { .. }) {
                    let _ = write!(out, "({})(", ttype.trim_end());
                    emit_expr(value, out);
                    out.push(')');
                    return;
                }
            }
            emit_expr(value, out);
        }
        CgExprData::Cast { target_type, expr } => {
            let _ = write!(out, "({})", target_type);
            emit_expr(expr, out);
        }
        CgExprData::Call { name, args, vtable_class, alt_vtable_classes, is_class_method, is_super, sel_const_name, method_index: _ } => {
            if *is_super {
                let sel = sel_const_name.as_deref().unwrap_or("0");
                let cls_flat = vtable_class.as_deref().map(|c| name_flat(c)).unwrap_or_else(|| "NPObject".to_string());
                // Super call: direct vtable instance access (typed struct member).
                // Uses the superclass's vtable instance, NOT self->isa->superclass,
                // to avoid infinite recursion with subclass runtime type.
                let _ = write!(out, "(&nupa_{}_vtable_inst)->{}(", cls_flat, name);
                if !args.is_empty() { emit_expr(&args[0], out); }
                let _ = write!(out, ", {}", sel);
                for (i, arg) in args[1..].iter().enumerate() {
                    out.push_str(", ");
                    let param_idx = i + 2;
                    if let Some(pt) = get_vtable_param_type(name, param_idx) {
                        if pt.ends_with('*') && !pt.trim_start().starts_with("const char") && !pt.trim_start().starts_with("char ") {
                            let _ = write!(out, "({})(", pt);
                            emit_expr(arg, out);
                            out.push_str(")");
                            continue;
                        }
                    }
                    emit_expr(arg, out);
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
                        for (i, arg) in args[1..].iter().enumerate() {
                            out.push_str(", ");
                            let param_idx = i + 2;
                            if let Some(pt) = get_vtable_param_type(name, param_idx) {
                                if pt.ends_with('*') && !pt.trim_start().starts_with("const char") && !pt.trim_start().starts_with("char ") {
                                    let _ = write!(out, "({})(", pt);
                                    emit_expr(arg, out);
                                    out.push_str(")");
                                    continue;
                                }
                            }
                            emit_expr(arg, out);
                        }
                    }
                    out.push(')');
                } else {
                    // Instance method: uniform vtable member access through isa.
                    // ((struct nupa_vtable *)receiver->isa->vtable)->method(args)
                    let sel = sel_const_name.as_deref().unwrap_or("0");
                    let is_simple = args.first().map_or(false, |a| matches!(a.kind, CgExprKind::Ident));
                    if is_simple && !args.is_empty() {
                        // Simple receiver — inline (avoids temp variable)
                        let _ = write!(out, "((struct nupa_vtable *)(");
                        emit_expr(&args[0], out);
                        let _ = write!(out, "->isa->vtable))->{}(", name);
                        // Cast receiver to NPObject* for the function call
                        let is_self = matches!(&args[0].data, CgExprData::Ident(s) if s == "self");
                        if !is_self {
                            out.push_str("(NPObject *)(");
                            emit_expr(&args[0], out);
                            out.push_str(")");
                        } else {
                            emit_expr(&args[0], out);
                        }
                        let _ = write!(out, ", {}", sel);
                        for (i, arg) in args[1..].iter().enumerate() {
                            out.push_str(", ");
                            // params: [NPObject* (0), SEL (1), user1 (2), user2 (3), ...]
                            // args[1+] corresponds to params[2+]
                            let param_idx = i + 2;
                            if let Some(pt) = get_vtable_param_type(name, param_idx) {
                                if pt.ends_with('*') && !pt.starts_with("const char") {
                                    let _ = write!(out, "({})(", pt);
                                    emit_expr(arg, out);
                                    out.push_str(")");
                                    continue;
                                }
                            }
                            emit_expr(arg, out);
                        }
                        out.push(')');
                    } else {
                        // Complex or empty receiver: temp variable
                        let tid = next_temp_id();
                        let _ = write!(out, "({{ NPObject *__nupa_tmp_{} = ((NPObject *)(", tid);
                        if !args.is_empty() {
                            emit_expr(&args[0], out);
                            out.push_str(")");
                        } else { out.push_str("0)"); }
                        let _ = write!(out, "); __nupa_tmp_{} ? ((struct nupa_vtable *)__nupa_tmp_{}->isa->vtable)->{}(", tid, tid, name);
                        let _ = write!(out, "__nupa_tmp_{}", tid);
                        let _ = write!(out, ", {}", sel);
                        for (i, arg) in args[1..].iter().enumerate() {
                            out.push_str(", ");
                            let param_idx = i + 2;
                            if let Some(pt) = get_vtable_param_type(name, param_idx) {
                                if pt.ends_with('*') && !pt.trim_start().starts_with("const char") && !pt.trim_start().starts_with("char ") {
                                    let _ = write!(out, "({})(", pt);
                                    emit_expr(arg, out);
                                    out.push_str(")");
                                    continue;
                                }
                            }
                            emit_expr(arg, out);
                        }
                        out.push_str(") : 0; })");
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
        CgStmtData::Decl { decl_type, name, init, array_suffix, is_static, is_weak, is_block, next } => {
            if *is_block {
                let byref_name = format!("__nupa_byref_{}", name);
                let _ = write!(out, "{}struct {} {{\n", ind, byref_name);
                let _ = write!(out, "{}    void *__isa;\n", ind);
                let _ = write!(out, "{}    struct {} *__forwarding;\n", ind, byref_name);
                let _ = write!(out, "{}    int __flags;\n", ind);
                let _ = write!(out, "{}    {} __value;\n", ind, decl_type);
                let _ = write!(out, "{}}};\n", ind);
                let _ = write!(out, "{}struct {} {} = {{\n", ind, byref_name, name);
                let _ = write!(out, "{}    .__forwarding = &{},\n", ind, name);
                let _ = write!(out, "{}    .__flags = 0,\n", ind);
                if let Some(i) = init {
                    let _ = write!(out, "{}    .__value = ", ind);
                    emit_expr(i, out);
                    out.push_str(",\n");
                }
                let _ = write!(out, "{}}};\n", ind);
                return;
            }
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
                    out.push_str(&ind);
                    if !args.is_empty() {
                            let tid = next_temp_id();
                            // Emit: NPObject *__nupa_tmp_N = receiver;
                            let _ = write!(out, "NPObject *__nupa_tmp_{} = (", tid);
                            emit_expr(&args[0], out);
                            out.push_str(");\n");
                            out.push_str(&ind);
                            if *is_static { out.push_str("static "); }
                            out.push_str(decl_type);
                            out.push(' ');
                            out.push_str(decl_var_name);
                            if let Some(suffix) = array_suffix { out.push_str(suffix); }
                            // Uniform vtable member dispatch with (SubClass*) cast for concrete class pointers
                            let needs_cast = decl_type.ends_with(" *") && decl_type != "NPObject *";
                            if needs_cast {
                                let _ = write!(out, " = ({})(", decl_type.trim_end());
                            } else {
                                out.push_str(" = ");
                            }
                            let _ = write!(out, "((struct nupa_vtable *)__nupa_tmp_{}->isa->vtable)->{}(", tid, method_name);
                            let _ = write!(out, "__nupa_tmp_{}", tid);
                        let _ = write!(out, ", {}", sel_const_name.as_deref().unwrap_or("0"));
                        for arg in &args[1..] {
                            out.push_str(", ");
                            emit_expr(arg, out);
                        }
                        if needs_cast {
                            out.push_str(")");
                        }
                        out.push_str(");\n");
                    } else {
                        if *is_static { out.push_str("static "); }
                        out.push_str(decl_type);
                        out.push(' ');
                        out.push_str(name);
                        if let Some(suffix) = array_suffix { out.push_str(suffix); }
                        let _ = write!(out, " = ((struct nupa_vtable *)0)->{}(", method_name);
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
                    // When init has type NPObject * but decl_type is a subclass pointer,
                    // add an explicit cast to avoid -Wincompatible-pointer-types.
                    let needs_cast = decl_type.ends_with(" *") && decl_type != "NPObject *"
                        && !is_block_type
                        && matches!(i.data, CgExprData::Call { .. });
                    let needs_reverse_cast = decl_type == "NPObject *"
                        && i.type_str.as_ref().map_or(false, |t| t.ends_with(" *") && t != "NPObject *");
                    let needs_self_cast = decl_type.ends_with(" *") && decl_type != "NPObject *"
                        && i.type_str.as_deref() == Some("NPObject *");
                    if needs_cast {
                        let _ = write!(out, " = ({})(", decl_type.trim_end());
                        emit_expr(i, out);
                        out.push_str(")");
                    } else if needs_reverse_cast {
                        let _ = write!(out, " = (NPObject *)(");
                        emit_expr(i, out);
                        out.push_str(")");
                    } else if needs_self_cast {
                        let _ = write!(out, " = ({})(", decl_type.trim_end());
                        emit_expr(i, out);
                        out.push_str(")");
                    } else {
                        out.push_str(" = ");
                        emit_expr(i, out);
                    }
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
        CgDeclData::Variable { var_type, init, is_static, is_const, is_block, next, .. } => {
            if *is_block {
                let byref_name = format!("__nupa_byref_{}", d.name);
                let _ = write!(out, "struct {} {{\n", byref_name);
                out.push_str("    void *__isa;\n");
                let _ = write!(out, "    struct {} *__forwarding;\n", byref_name);
                out.push_str("    int __flags;\n");
                let _ = write!(out, "    {} __value;\n", var_type);
                let _ = write!(out, "}} {};\n", byref_name);
                let _ = write!(out, "struct {} {} = {{\n", byref_name, d.name);
                let _ = write!(out, "    .__forwarding = &{},\n", d.name);
                out.push_str("    .__flags = 0,\n");
                if let Some(i) = init {
                    out.push_str("    .__value = ");
                    emit_expr(i, out);
                    out.push_str(",\n");
                }
                out.push_str("};\n");
                return;
            }
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
                    // When init has type NPObject * but var_type is a subclass pointer,
                    // add an explicit cast to avoid -Wincompatible-pointer-types.
                    let needs_cast = var_type.ends_with(" *") && var_type != "NPObject *"
                        && i.type_str.as_deref() == Some("NPObject *");
                    if needs_cast {
                        let _ = write!(out, " = ({})(", var_type);
                        emit_expr(i, out);
                        out.push_str(")");
                    } else {
                        out.push_str(" = ");
                        emit_expr(i, out);
                    }
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
    let has_string_h = c_headers.iter().any(|h| h.contains("string.h"));
    if !has_string_h {
        out.push_str("#include <string.h>\n");
    }
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

    let any_has_instance = unit.classes.iter().any(|cm| cm.method_names.iter().zip(&cm.is_class_methods).any(|(_, &ic)| !ic));

    // Forward-declare vtable structs (per-class typed vtables, plus meta vtable)
    {
        if any_has_instance {
            let _ = write!(out, "struct nupa_vtable;\n");
        }
        for cm in &unit.classes {
            let flat_cn = name_flat(&cm.class_name);
            let has_class_methods = cm.is_class_methods.iter().any(|&c| c);
            let has_super = cm.super_name.is_some();
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

    // Forward declarations + typedefs for class types
    for cm in &unit.classes {
        let fc = name_flat(&cm.class_name);
        if fc == "__nupa_root" || fc == "NPObject" {
            // Emit full struct definitions with include guard so that if
            // runtime.h (which already defines them) is included first,
            // these are silently skipped.  If runtime.h is NOT available,
            // these definitions ensure the generated code compiles.
            // Guards must match those used in include/nupa/runtime.h.
            let guard = if fc == "__nupa_root" {
                "__NUPA_ROOT_DEFINED"
            } else {
                "NPOBJECT_DEFINED"
            };
            let _ = writeln!(out, "#ifndef {}", guard);
            let _ = writeln!(out, "#define {}", guard);
            let _ = writeln!(out, "struct {} {{", fc);
            if fc == "__nupa_root" {
                let _ = writeln!(out, "    struct NPClass *isa;");
            } else {
                let _ = writeln!(out, "    struct NPClass *isa;");
            }
            let _ = writeln!(out, "    uint32_t retain_count;");
            let _ = writeln!(out, "}};");
            let _ = writeln!(out, "typedef struct {} {};", fc, fc);
            let _ = writeln!(out, "#endif");
        } else {
            let _ = write!(out, "struct {};\n", fc);
            let _ = write!(out, "typedef struct {} {};\n", fc, fc);
        }
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
    // Block typedefs with duplicate signatures are deduplicated: only the first
    // occurrence emits a full typedef; subsequent same-signature blocks are
    // either skipped (if alias already exists) or emit only an alias line.
    let mut seen_block_sigs: std::collections::HashSet<String> = std::collections::HashSet::new();
        for decl in &unit.decls {
            if let CgDeclData::Typedef { ref alias, ref type_str, ref struct_fields } = decl.data {
                if struct_fields.is_empty() {
                // Block type dedup: skip full typedef if same signature seen
                if type_str.contains("(^") {
                    let sig = block_type_signature_key(type_str);
                    if !seen_block_sigs.insert(sig) {
                        continue;
                    }
                }
                if *alias == *type_str {
                    continue;
                }
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
        if let CgDeclData::Variable { ref var_type, ref init, is_static, is_const, is_block, .. } = decl.data {
            if is_block {
                let byref_name = format!("__nupa_byref_{}", decl.name);
                let _ = write!(out, "struct {} {{\n", byref_name);
                out.push_str("    void *__isa;\n");
                let _ = write!(out, "    struct {} *__forwarding;\n", byref_name);
                out.push_str("    int __flags;\n");
                let _ = write!(out, "    {} __value;\n", var_type);
                let _ = write!(out, "}} {};\n", byref_name);
                let _ = write!(out, "struct {} {} = {{\n", byref_name, decl.name);
                let _ = write!(out, "    .__forwarding = &{},\n", decl.name);
                out.push_str("    .__flags = 0,\n");
                if let Some(i) = init {
                    out.push_str("    .__value = ");
                    emit_expr(i, &mut out);
                    out.push_str(",\n");
                }
                out.push_str("};\n");
                continue;
            }
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

    // VTable struct definitions (per-class, type-safe).
    // Uniform vtable: a single struct with all instance methods as typed function pointers.
    // All vtable instances use this struct type, making dispatch via member access
    // type-safe regardless of which class the receiver belongs to.
    if any_has_instance {
        let _ = write!(out, "struct nupa_vtable {{\n");
        for mname in &unit.global_instance_method_names {
            let (_, ptr_type) = METHOD_METADATA.get().unwrap().get(mname.as_str()).unwrap();
            // ptr_type is "return_type (*)(params)". Insert mname after the "*".
            if let Some(paren) = ptr_type.rfind("(*)") {
                let before = &ptr_type[..paren + 2]; // "return_type (*"
                let after = &ptr_type[paren + 2..];  // ")(params)"
                let _ = write!(out, "    {}{}{};\n", before, mname, after);
            } else {
                let _ = write!(out, "    {} {};\n", ptr_type, mname);
            }
        }
        out.push_str("};\n\n");
    }

    // Meta vtable struct definitions (per-class, for class methods)
    for cm in &unit.classes {
        if !cm.method_names.is_empty() {
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
        // Walk superclass chain and emit ancestor ivars (flat, not embedded).
        // Each non-root struct starts with isa+retain_count (matching __nupa_root)
        // followed by all ancestor ivars, then this class's own ivars.
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

    // Instance vtable instances (per-class typed, with designated initializers)
    for cm in &unit.classes {
        let flat_cn = name_flat(&cm.class_name);
        let _ = write!(out, "struct nupa_vtable nupa_{}_vtable_inst = {{\n", flat_cn);
        for mname in &unit.global_instance_method_names {
            if let Some(pos) = cm.method_names.iter().position(|n| n == mname) {
                if !cm.is_class_methods[pos] {
                    let owner = cm.method_owners.get(pos).cloned().unwrap_or_else(|| flat_cn.clone());
                    let (_, ptr_type) = METHOD_METADATA.get().unwrap().get(mname.as_str()).unwrap();
                    let _ = write!(out, "    .{} = ({}){}_{},\n", mname, ptr_type, owner, mname);
                } else {
                    let _ = write!(out, "    .{} = NULL,\n", mname);
                }
            } else {
                let _ = write!(out, "    .{} = NULL,\n", mname);
            }
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
                out.push_str(&format!("        .superclass = &nupa_{}_class,\n", name_flat(sup)));
            } else {
                out.push_str("        .superclass = NULL,\n");
            }
            out.push_str(&format!("        .instance_size = sizeof(struct {}),\n", name_flat(&cm.class_name)));
            if cm.method_names.is_empty() {
                if let Some(ref sup) = cm.super_name {
                    out.push_str(&format!("        .vtable = &nupa_{}_vtable_inst,\n", name_flat(sup)));
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
        // Skip enum, typedef, and variable decls — they are already emitted
        // in the forward-declaration phase above. Emitting again here would
        // cause `redefinition` errors.
        if matches!(decl.data, CgDeclData::Enum { .. }) { continue; }
        if matches!(decl.data, CgDeclData::Typedef { .. }) { continue; }
        if matches!(decl.data, CgDeclData::Variable { .. }) { continue; }
        emit_decl(decl, &mut out);
        out.push('\n');
    }

    out
}

pub fn emit_unit(unit: &CgUnit) -> String {
    emit_unit_with_headers(unit, &[], &[])
}