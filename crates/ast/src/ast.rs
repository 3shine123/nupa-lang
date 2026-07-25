use nupa_cst::{CstParam, TypePrim};

// ─── Type node ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AstType {
    pub prim: TypePrim,
    pub is_pointer: bool,
    pub is_const: bool,
    pub is_block: bool,
    pub is_array: bool,
    pub is_struct: bool,
    pub is_unsigned: bool,
    pub array_size: i32,
    /// Symbolic array size identifier when the source used a macro/enum
    /// constant (e.g. `FSNode *_children[MAX_CHILDREN];`). When Some, codegen
    /// emits `T[MAX_CHILDREN]` rather than `T[]` (flexible array member),
    /// which C forbids outside the trailing field.
    pub array_size_name: Option<String>,
    pub subtype: Option<Box<AstType>>,
    pub block_params: Option<Box<AstType>>,
    pub block_name: Option<String>,
    pub next: Option<Box<AstType>>,
    pub type_args: Vec<AstType>,
    pub name: Option<String>,
    pub class_ref: Option<String>,
    pub protocol_ref: Option<String>,
    pub protocol_refs: Vec<String>,
}

impl AstType {
    pub fn new(prim: TypePrim) -> Self {
        AstType {
            prim, is_pointer: false, is_const: false, is_block: false,
            is_array: false, is_struct: false, is_unsigned: false, array_size: 0,
            subtype: None, block_params: None, block_name: None, next: None,
            type_args: Vec::new(), name: None,
            class_ref: None, protocol_ref: None, protocol_refs: Vec::new(),
            array_size_name: None,
        }
    }
}

// ─── Expression kinds ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AstExprKind {
    Int, Float, Char, String, Bool,
    Nil, Null, Self_, Super, Selector,
    VarRef, IvarRef, PropRef,
    MsgSend, FuncCall,
    Unary, Binary, Assign, Cast,
    BlockLit, ArrayLit, DictLit,
    Subscript, Comma, Sizeof, Ternary,
}

#[derive(Debug, Clone)]
pub struct AstExpr {
    pub kind: AstExprKind,
    pub expr_type: Option<Box<AstType>>,
    pub line: usize,
    pub col: usize,
    pub data: AstExprData,
}

#[derive(Debug, Clone)]
pub enum AstExprData {
    Int(i64),
    Float(f64),
    Char(u8),
    String(String),
    Bool(bool),
    VarRef { sym: Option<String>, name: String },
    IvarRef { ivar: Option<String>, cls: Option<String>, obj: Box<AstExpr> },
    PropRef { prop: Option<String>, cls: Option<String>, obj: Box<AstExpr>, name: String, is_arrow: bool },
    MsgSend {
        receiver: Box<AstExpr>,
        method: Option<String>,
        vtable_index: i32,
        is_class_method: bool,
        is_super: bool,
        super_name: Option<String>,
        selector: String,
        args: Vec<AstExpr>,
    },
    FuncCall {
        func: Option<String>,
        name: String,
        callee: Option<Box<AstExpr>>,
        args: Vec<AstExpr>,
    },
    Unary { op: i32, operand: Box<AstExpr>, is_postfix: bool },
    Binary { op: i32, left: Box<AstExpr>, right: Box<AstExpr> },
    Assign { target: Box<AstExpr>, value: Box<AstExpr> },
    Cast { target_type: AstType, expr: Box<AstExpr> },
    ArrayLit(Vec<AstExpr>),
    DictLit { keys: Vec<AstExpr>, values: Vec<AstExpr> },
    Comma(Vec<AstExpr>),
    Subscript { object: Box<AstExpr>, key: Box<AstExpr> },
    Sizeof { type_expr: AstType, expr: Option<Box<AstExpr>> },
    Block { params: Option<Box<CstParam>>, return_type: Option<Box<AstType>>, body: Option<Box<AstStmt>> },
    Ternary { cond: Box<AstExpr>, then: Box<AstExpr>, else_: Box<AstExpr> },
    Selector(String),
}

// ─── Statement kinds ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AstStmtKind {
    Expr, Compound, If, Switch, Case, Default,
    While, Do, For, ForIn,
    Break, Continue, Return, Goto, Label,
    Throw, Try, Catch, Finally,
    Synchronized, Autoreleasepool, Decl,
}

#[derive(Debug, Clone)]
pub struct AstStmt {
    pub kind: AstStmtKind,
    pub line: usize,
    pub col: usize,
    pub data: AstStmtData,
}

#[derive(Debug, Clone)]
pub enum AstStmtData {
    Expr(AstExpr),
    Compound(Vec<AstStmt>),
    If { cond: Box<AstExpr>, then: Box<AstStmt>, else_: Option<Box<AstStmt>> },
    Switch { expr: Box<AstExpr>, body: Box<AstStmt> },
    Case { value: Box<AstExpr>, body: Box<AstStmt> },
    Default(Box<AstStmt>),
    While { cond: Box<AstExpr>, body: Box<AstStmt> },
    Do { body: Box<AstStmt>, cond: Box<AstExpr> },
    For { init: Option<Box<AstStmt>>, cond: Option<Box<AstExpr>>, incr: Option<Box<AstExpr>>, body: Box<AstStmt> },
    ForIn { var: Box<AstExpr>, collection: Box<AstExpr>, body: Box<AstStmt> },
    Return(Option<Box<AstExpr>>),
    Goto(String),
    Label(String),
    Throw(Option<Box<AstExpr>>),
    Try { try_block: Box<AstStmt>, catches: Vec<AstStmt>, finally_block: Option<Box<AstStmt>> },
    Catch { param: CstParam, body: Box<AstStmt> },
    Finally(Box<AstStmt>),
    Synchronized { lock: Box<AstExpr>, body: Box<AstStmt> },
    Autoreleasepool(Box<AstStmt>),
    Decl(AstDecl),
}

// ─── Declaration kinds ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AstDeclKind {
    Class, Method, Ivar, Property,
    Function, Variable, Protocol,
    Typedef, Struct, Union, Enum, Namespace,
}

#[derive(Debug, Clone)]
pub struct AstDecl {
    pub kind: AstDeclKind,
    pub name: Option<String>,
    pub line: usize,
    pub col: usize,
    pub data: AstDeclData,
}

#[derive(Debug, Clone)]
 pub enum AstDeclData {
     Class {
         cls_sym: Option<String>,
         super_name: Option<String>,
         methods: Vec<AstDecl>,
         ivars: Vec<AstDecl>,
         properties: Vec<AstDecl>,
         impl_vars: Vec<AstDecl>,
     },
     Method {
         method_sym: Option<String>,
         is_class_method: bool,
         return_type: Option<Box<AstType>>,
         params: Option<Box<CstParam>>,
         body: Option<Box<AstStmt>>,
     },
     Ivar {
         ivar_sym: Option<String>,
         ivar_type: Option<Box<AstType>>,
     },
     Property {
         prop_sym: Option<String>,
         prop_type: Option<Box<AstType>>,
         getter: Option<String>,
         setter: Option<String>,
         is_readonly: bool,
         is_weak: bool,
         is_assign: bool,
         is_retain: bool,
         is_copy: bool,
         is_nonatomic: bool,
         is_dynamic: bool,
     },
    Function {
        func_sym: Option<String>,
        return_type: Option<Box<AstType>>,
        params: Option<Box<CstParam>>,
        body: Option<Box<AstStmt>>,
    },
    Variable {
        var_type: Option<Box<AstType>>,
        init: Option<Box<AstExpr>>,
        is_static: bool,
        is_extern: bool,
        is_const: bool,
        is_block_qual: bool,
        is_weak: bool,
        next: Option<Box<AstDecl>>,
    },
    Typedef {
        aliased_type: Option<Box<AstType>>,
        struct_fields: Vec<AstDecl>,
    },
    Aggregate {
        fields: Vec<AstDecl>,
    },
    Enum {
        members: Vec<String>,
        values: Vec<AstExpr>,
    },
    Namespace(Vec<AstDecl>),
}

// ─── Translation unit ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AstUnit {
    pub decls: Vec<AstDecl>,
    pub filename: String,
}