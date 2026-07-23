use nupa_cst::*;
use nupa_ast::*;
use nupa_symbol::symbol::*;

pub struct Elaborator {
    pub symtab: Option<SymbolTable>,
    pub result: Option<AstUnit>,
    pub current_class_sym: Option<String>,
    pub has_error: bool,
    pub err_msg: String,
    pub ns_prefix: String,
}

impl Elaborator {
    pub fn new(symtab: Option<SymbolTable>) -> Self {
        Elaborator {
            symtab, result: None, current_class_sym: None,
            has_error: false, err_msg: String::new(), ns_prefix: String::new(),
        }
    }

    pub fn has_error(&self) -> bool { self.has_error }
    pub fn last_error(&self) -> &str { &self.err_msg }

    fn ns_fqn(&self, name: &str) -> String {
        // Always check alias list first (applies to all @using forms)
        if let Some(ref st) = self.symtab {
            if let Some(entry) = st.find_using(name) {
                if !entry.is_namespace {
                    return entry.fqn.clone();
                }
            }
            // Check @using namespace entries — try <ns>::<name>
            if !name.contains("::") {
                for entry in &st.using_list {
                    if entry.is_namespace {
                        let prefixed = format!("{}::{}", entry.fqn, name);
                        if st.find_class(&prefixed).is_some() || st.find_protocol(&prefixed).is_some() {
                            return prefixed;
                        }
                    }
                }
            }
        }
        if name.contains("::") || self.ns_prefix.is_empty() {
            name.to_string()
        } else {
            format!("{}{}", self.ns_prefix, name)
        }
    }

    fn resolve_cst_type_name(ct: &mut CstType, ns_prefix: &str, symtab: &Option<SymbolTable>) {
        if let Some(ref name) = ct.name.clone() {
            if !name.contains("::") {
                // Check @using namespace entries first
                if let Some(ref st) = symtab {
                    for entry in &st.using_list {
                        if entry.is_namespace {
                            let fqn = format!("{}::{}", entry.fqn, name);
                            if st.find_type(&fqn).is_some() || st.find_class(&fqn).is_some() || st.find_protocol(&fqn).is_some() {
                                ct.name = Some(fqn);
                                break;
                            }
                        }
                    }
                }
                // Fall back to ns_prefix (set by @namespace blocks)
                if ct.name.as_deref() == Some(name) && !ns_prefix.is_empty() {
                    let fqn = format!("{}{}", ns_prefix, name);
                    if let Some(ref st) = symtab {
                        if st.find_type(&fqn).is_some() || st.find_class(&fqn).is_some() || st.find_protocol(&fqn).is_some() {
                            ct.name = Some(fqn);
                        }
                    }
                }
            }
        }
        if let Some(ref mut sub) = ct.subtype {
            Self::resolve_cst_type_name(sub, ns_prefix, symtab);
        }
        if let Some(ref mut bp) = ct.block_params {
            Self::resolve_cst_type_name(bp, ns_prefix, symtab);
        }
        for ta in &mut ct.type_args {
            Self::resolve_cst_type_name(ta, ns_prefix, symtab);
        }
    }

    fn convert_type(&mut self, ct: &CstType) -> Option<AstType> {
        let mut at = AstType::new(ct.prim);
        at.is_pointer = ct.is_pointer; at.is_const = ct.is_const; at.is_block = ct.is_block;
        at.is_array = ct.is_array; at.is_struct = ct.is_struct; at.array_size = ct.array_size;
        at.array_size_name = ct.array_size_name.clone();
        at.name = ct.name.clone();
        at.block_name = ct.block_name.clone();
        at.subtype = ct.subtype.as_ref().and_then(|s| self.convert_type(s)).map(Box::new);
        at.next = ct.next.as_ref().and_then(|n| self.convert_type(n)).map(Box::new);
        at.block_params = ct.block_params.as_ref().and_then(|b| self.convert_type(b)).map(Box::new);
        at.protocol_refs = ct.protocols.clone();
        at.type_args = ct.type_args.iter().filter_map(|a| self.convert_type(a)).collect();
        // Resolve class/protocol refs from symbol table (like C's convert_type_node)
        if let Some(ref st) = self.symtab {
            if let Some(ref name) = at.name {
                if at.prim == TypePrim::Named {
                    // Check if this name is a @using alias with type info
                    // (e.g. `SecTunnel` → `Network::Security::QuantumPacket<...>*`)
                    if let Some(entry) = st.find_using(name) {
                        if !entry.is_namespace {
                            at.name = Some(entry.fqn.clone());
                            at.is_pointer = entry.ptr_level > 0 || ct.is_pointer;
                            // Handle `@using Alias = id<Protocol>` → convert to TypePrim::Id
                            if entry.fqn.starts_with("id<") && entry.fqn.ends_with(">") {
                                let proto = &entry.fqn[3..entry.fqn.len()-1];
                                at.prim = TypePrim::Id;
                                at.protocol_refs = vec![proto.to_string()];
                                at.name = None;
                            } else if let Some(cls) = st.find_class(&entry.fqn) {
                                at.class_ref = Some(cls.name.clone());
                            }
                        }
                    } else {
                        let fqn = self.ns_fqn(name);
                        if let Some(cls) = st.find_class(&fqn).or_else(|| st.find_class(name)) {
                            at.class_ref = Some(cls.name.clone());
                        }
                        if let Some(proto) = st.find_protocol(&fqn).or_else(|| st.find_protocol(name)) {
                            at.protocol_ref = Some(proto.name.clone());
                        }
                    }
                }
            }
        }
        Some(at)
    }

    fn convert_expr(&mut self, ce: &CstExpr) -> Option<AstExpr> {
        let line = ce.line; let col = ce.col;
        let ae = match &ce.data {
            CstExprData::Integer(val) => AstExpr { kind: AstExprKind::Int, expr_type: None, line, col, data: AstExprData::Int(*val) },
            CstExprData::Float(val) => AstExpr { kind: AstExprKind::Float, expr_type: None, line, col, data: AstExprData::Float(*val) },
            CstExprData::String(s) => AstExpr { kind: AstExprKind::String, expr_type: None, line, col, data: AstExprData::String(s.clone()) },
            CstExprData::Char(val) => AstExpr { kind: AstExprKind::Char, expr_type: None, line, col, data: AstExprData::Char(*val) },
            CstExprData::Bool(val) => AstExpr { kind: AstExprKind::Bool, expr_type: None, line, col, data: AstExprData::Bool(*val) },
            CstExprData::Ident(name) => {
                match ce.kind {
                    CstExprKind::Nil => AstExpr { kind: AstExprKind::Nil, expr_type: None, line, col, data: AstExprData::VarRef { sym: None, name: "nil".into() } },
                    CstExprKind::Null => AstExpr { kind: AstExprKind::Null, expr_type: None, line, col, data: AstExprData::VarRef { sym: None, name: "NULL".into() } },
                    CstExprKind::Self_ => AstExpr { kind: AstExprKind::Self_, expr_type: None, line, col, data: AstExprData::VarRef { sym: None, name: "self".into() } },
                    CstExprKind::Super => AstExpr { kind: AstExprKind::Super, expr_type: None, line, col, data: AstExprData::VarRef { sym: None, name: "super".into() } },
                    CstExprKind::Cmd => AstExpr { kind: AstExprKind::VarRef, expr_type: None, line, col, data: AstExprData::VarRef { sym: None, name: "_cmd".into() } },
                    _ => self.convert_ident_expr(line, col, name).unwrap_or_else(|| AstExpr { kind: AstExprKind::VarRef, expr_type: None, line, col, data: AstExprData::VarRef { sym: None, name: name.clone() } }),
                }
            }
            CstExprData::Selector(s) => AstExpr { kind: AstExprKind::Selector, expr_type: None, line, col, data: AstExprData::Selector(s.clone()) },
            CstExprData::Protocol(s) => AstExpr { kind: AstExprKind::Selector, expr_type: None, line, col, data: AstExprData::Selector(s.clone()) },
            CstExprData::Encode(ty) => AstExpr {
                kind: AstExprKind::Sizeof, expr_type: None, line, col,
                data: AstExprData::Sizeof { type_expr: self.convert_type(ty).unwrap_or_else(|| AstType::new(TypePrim::Void)), expr: None },
            },
            CstExprData::Sizeof { type_expr, expr } => AstExpr {
                kind: AstExprKind::Sizeof, expr_type: None, line, col,
                data: AstExprData::Sizeof {
                    type_expr: self.convert_type(type_expr).unwrap_or_else(|| AstType::new(TypePrim::Void)),
                    expr: expr.as_ref().map(|e| Box::new(self.convert_expr(e).unwrap_or_else(make_int_expr))),
                },
            },
            CstExprData::Typeof(ty) => AstExpr {
                kind: AstExprKind::Sizeof, expr_type: None, line, col,
                data: AstExprData::Sizeof { type_expr: self.convert_type(ty).unwrap_or_else(|| AstType::new(TypePrim::Void)), expr: None },
            },
            CstExprData::Paren(e) => self.convert_expr(e).unwrap_or_else(make_int_expr),
            CstExprData::NumberLit(e) => self.convert_expr(e).unwrap_or_else(make_int_expr),
            CstExprData::Message { receiver, selector, args } => {
                let mut is_class_method = false;
                let mut is_super = false;
                let mut super_name = None;
                // Check if receiver is a class name (like C's convert_expr lines 207-216)
                let receiver_class_name = if let Some(ref st) = self.symtab {
                    if let CstExprData::Ident(ref name) = receiver.data {
                        let fqn = self.ns_fqn(name);
                        if st.find_class(&fqn).is_some() {
                            is_class_method = true;
                            Some(fqn)
                        } else if st.find_class(name).is_some() {
                            is_class_method = true;
                            None
                        } else {
                            None
                        }
                    } else { None }
                } else { None };
                // Handle super receiver (like C's convert_expr lines 217-222)
                if receiver.kind == CstExprKind::Super {
                    is_super = true;
                    if let Some(ref st) = self.symtab {
                        if let Some(ref cls_name) = self.current_class_sym {
                            if let Some(csym) = st.find_class(cls_name) {
                                if let SymbolData::Class { ref superclass, .. } = csym.data {
                                    super_name = superclass.clone();
                                }
                            }
                        }
                    }
                    // If super_name is still None (e.g. @implementation doesn't have superclass),
                    // walk the class hierarchy to find it
                    if super_name.is_none() {
                        if let Some(ref st) = self.symtab {
                            if let Some(ref cls_name) = self.current_class_sym {
                                if let Some(csym) = st.find_class(cls_name) {
                                    if let SymbolData::Class { ref superclass, .. } = csym.data {
                                        super_name = superclass.clone();
                                    }
                                }
                            }
                        }
                    }
                }
                let mut ae = AstExpr {
                    kind: AstExprKind::MsgSend, expr_type: None, line, col,
                    data: AstExprData::MsgSend {
                        receiver: Box::new(if let Some(ref fqn) = receiver_class_name {
                            // Use the resolved class name (with namespace prefix) for the receiver
                            AstExpr { kind: AstExprKind::VarRef, expr_type: None, line, col, data: AstExprData::VarRef { sym: None, name: fqn.clone() } }
                        } else {
                            self.convert_expr(receiver).unwrap_or_else(make_self_expr)
                        }),
                        method: None, vtable_index: -1,
                        is_class_method, is_super, super_name,
                        selector: selector.clone(),
                        args: args.iter().filter_map(|a| self.convert_expr(a)).collect(),
                    },
                };
                ae
            }
            CstExprData::Dot { object, property } => {
                self.convert_dot_expr(line, col, object, property, false)
            }
            CstExprData::Arrow { object, property } => {
                self.convert_dot_expr(line, col, object, property, true)
            }
            CstExprData::Unary { op, operand, is_postfix } => {
                AstExpr { kind: AstExprKind::Unary, expr_type: None, line, col, data: AstExprData::Unary { op: *op, operand: Box::new(self.convert_expr(operand).unwrap_or_else(make_int_expr)), is_postfix: *is_postfix } }
            }
            CstExprData::Assign { target, value } => {
                AstExpr { kind: AstExprKind::Assign, expr_type: None, line, col, data: AstExprData::Assign { target: Box::new(self.convert_expr(target).unwrap_or_else(make_int_expr)), value: Box::new(self.convert_expr(value).unwrap_or_else(make_int_expr)) } }
            }
            CstExprData::Binary { op, left, right } => {
                AstExpr { kind: AstExprKind::Binary, expr_type: None, line, col, data: AstExprData::Binary { op: *op, left: Box::new(self.convert_expr(left).unwrap_or_else(make_int_expr)), right: Box::new(self.convert_expr(right).unwrap_or_else(make_int_expr)) } }
            }
            CstExprData::Ternary { cond, true_expr, false_expr } => {
                AstExpr { kind: AstExprKind::Ternary, expr_type: None, line, col, data: AstExprData::Ternary { cond: Box::new(self.convert_expr(cond).unwrap_or_else(make_int_expr)), then: Box::new(self.convert_expr(true_expr).unwrap_or_else(make_int_expr)), else_: Box::new(self.convert_expr(false_expr).unwrap_or_else(make_int_expr)) } }
            }
            CstExprData::Cast { target_type, expr } => {
                AstExpr { kind: AstExprKind::Cast, expr_type: None, line, col, data: AstExprData::Cast { target_type: self.convert_type(target_type).unwrap_or_else(|| AstType::new(TypePrim::Void)), expr: Box::new(self.convert_expr(expr).unwrap_or_else(make_int_expr)) } }
            }
            CstExprData::Call { callee, args } => {
                let args_list: Vec<AstExpr> = args.iter().filter_map(|a| self.convert_expr(a)).collect();
                let name = match callee.data {
                    CstExprData::Ident(ref n) => n.clone(),
                    _ => String::new(),
                };
                let converted_callee = self.convert_expr(callee);
                if let Some(ref ce) = converted_callee {
                    if matches!(ce.data, AstExprData::IvarRef { .. }) {
                        // Block invocation on ivar — store callee expression
                        AstExpr { kind: AstExprKind::FuncCall, expr_type: None, line, col,
                            data: AstExprData::FuncCall { func: None, name, callee: Some(Box::new(ce.clone())), args: args_list } }
                    } else {
                        let func_sym = self.symtab.as_ref()
                            .and_then(|st| st.lookup(&name))
                            .filter(|s| s.kind == SymbolKind::Function)
                            .map(|s| s.name.clone());
                        AstExpr { kind: AstExprKind::FuncCall, expr_type: None, line, col,
                            data: AstExprData::FuncCall { func: func_sym, name, callee: None, args: args_list } }
                    }
                } else {
                    AstExpr { kind: AstExprKind::FuncCall, expr_type: None, line, col,
                        data: AstExprData::FuncCall { func: None, name, callee: None, args: args_list } }
                }
            }
            CstExprData::Subscript { object, key } => {
                AstExpr { kind: AstExprKind::Subscript, expr_type: None, line, col, data: AstExprData::Subscript { object: Box::new(self.convert_expr(object).unwrap_or_else(make_int_expr)), key: Box::new(self.convert_expr(key).unwrap_or_else(make_int_expr)) } }
            }
            CstExprData::Comma(exprs) => {
                AstExpr { kind: AstExprKind::Comma, expr_type: None, line, col, data: AstExprData::Comma(exprs.iter().filter_map(|e| self.convert_expr(e)).collect()) }
            }
            CstExprData::ArrayLit(exprs) => {
                AstExpr { kind: AstExprKind::ArrayLit, expr_type: None, line, col, data: AstExprData::ArrayLit(exprs.iter().filter_map(|e| self.convert_expr(e)).collect()) }
            }
            CstExprData::DictLit { keys, values } => {
                AstExpr { kind: AstExprKind::DictLit, expr_type: None, line, col, data: AstExprData::DictLit { keys: keys.iter().filter_map(|k| self.convert_expr(k)).collect(), values: values.iter().filter_map(|v| self.convert_expr(v)).collect() } }
            }
            CstExprData::Block { params, return_type, body, .. } => {
                AstExpr { kind: AstExprKind::BlockLit, expr_type: None, line, col, data: AstExprData::Block { params: params.clone(), return_type: return_type.as_ref().and_then(|t| self.convert_type(t)).map(Box::new), body: body.as_ref().and_then(|b| self.convert_stmt(b)).map(Box::new) } }
            }
            CstExprData::InitList(exprs) => {
                AstExpr { kind: AstExprKind::ArrayLit, expr_type: None, line, col, data: AstExprData::ArrayLit(exprs.iter().filter_map(|e| self.convert_expr(e)).collect()) }
            }
        };
        Some(ae)
    }

    fn convert_ivar_ref(&mut self, line: usize, col: usize, name: &str) -> Option<AstExpr> {
        if let Some(ref st) = self.symtab {
            if let Some(ref cls_name) = self.current_class_sym {
                let mut cls = st.find_class(cls_name);
                while let Some(csym) = cls {
                    if let SymbolData::Class { ref ivars, .. } = csym.data {
                        if ivars.contains(&name.to_string()) {
                            return Some(AstExpr {
                                kind: AstExprKind::IvarRef, expr_type: None, line, col,
                                data: AstExprData::IvarRef {
                                    ivar: Some(name.to_string()),
                                    cls: Some(cls_name.clone()),
                                    obj: Box::new(AstExpr { kind: AstExprKind::Self_, expr_type: None, line, col, data: AstExprData::VarRef { sym: None, name: "self".into() } }),
                                },
                            });
                        }
                    }
                    // Walk superclass chain (like C's elaborator)
                    if let SymbolData::Class { ref superclass, .. } = csym.data {
                        cls = superclass.as_ref().and_then(|s| st.find_class(s));
                    } else {
                        cls = None;
                    }
                }
            }
        }
        None
    }

    fn convert_dot_expr(&mut self, line: usize, col: usize, object: &CstExpr, property: &str, is_arrow: bool) -> AstExpr {
        // Self/super access (like C's elaborator lines 231-257)
        if object.kind == CstExprKind::Self_ || object.kind == CstExprKind::Super {
            if let Some(st) = &self.symtab {
                if let Some(cls) = &self.current_class_sym {
                    if let Some(csym) = st.find_class(cls) {
                        if let SymbolData::Class { ref ivars, ref properties, .. } = csym.data {
                            for iv in ivars {
                                if iv == property {
                                    return AstExpr {
                                        kind: AstExprKind::IvarRef, expr_type: None, line, col,
                                        data: AstExprData::IvarRef { ivar: Some(property.to_string()), cls: Some(cls.clone()), obj: Box::new(AstExpr { kind: AstExprKind::Self_, expr_type: None, line, col, data: AstExprData::VarRef { sym: None, name: "self".into() } }) },
                                    };
                                }
                            }
                            for p in properties {
                                if p == property {
                                    // ObjC instances are always `Type *` (pointer to
                                    // struct) in the generated C, so property access
                                    // must emit `->` regardless of source `.`/`->`.
                                    return AstExpr {
                                        kind: AstExprKind::PropRef, expr_type: None, line, col,
                                        data: AstExprData::PropRef { prop: Some(property.to_string()), cls: Some(cls.clone()), obj: Box::new(AstExpr { kind: AstExprKind::Self_, expr_type: None, line, col, data: AstExprData::VarRef { sym: None, name: "self".into() } }), name: property.to_string(), is_arrow: true },
                                    };
                                }
                            }
                        }
                    }
                }
            }
        }
        // Non-self object access (like C's elaborator lines 259-353)
        if let Some(ref st) = self.symtab {
            if let CstExprData::Ident(ref obj_name) = object.data {
                let mut cls = None;
                // Try symtab lookup for the object variable
                if let Some(obj_sym) = st.lookup(obj_name) {
                    match &obj_sym.data {
                        SymbolData::Variable { var_type, .. } | SymbolData::Ivar { ivar_type: var_type, .. } => {
                            let mut t = var_type.as_ref().map(|t| &**t);
                            let mut found = false;
                            while let Some(tp) = t {
                                if let Some(ref cr) = tp.class_ref {
                                    cls = st.find_class(cr);
                                    found = true;
                                    break;
                                }
                                // Fallback: if class_ref is None but name is set, try the name directly
                                // and also try the ns_fqn-resolved name
                                if let Some(ref name) = tp.name {
                                    let fqn = self.ns_fqn(name);
                                    if let Some(found_cls) = st.find_class(&fqn).or_else(|| st.find_class(name)) {
                                        cls = Some(found_cls);
                                        found = true;
                                        break;
                                    }
                                }
                                if tp.is_pointer { t = tp.subtype.as_ref().map(|s| &**s); continue; }
                                break;
                            }
                            // If not found through the type chain, try the original type name
                            if !found {
                                if let Some(var_type) = var_type {
                                    if let Some(ref name) = var_type.name {
                                        let fqn = self.ns_fqn(name);
                                        if let Some(found_cls) = st.find_class(&fqn).or_else(|| st.find_class(name)) {
                                            cls = Some(found_cls);
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                // Try to find the property or ivar in the class
                if let Some(cls_sym) = cls {
                    if let SymbolData::Class { ref ivars, ref properties, .. } = cls_sym.data {
                        // Check if it's an ivar first
                        if ivars.contains(&property.to_string()) {
                            return AstExpr {
                                kind: AstExprKind::IvarRef, expr_type: None, line, col,
                                data: AstExprData::IvarRef { ivar: Some(property.to_string()), cls: Some(cls_sym.name.clone()), obj: Box::new(self.convert_expr(object).unwrap_or_else(make_self_expr)) },
                            };
                        }
                        // Then check if it's a property
                        if properties.contains(&property.to_string()) {
                            // ObjC instances are always `Type *` in generated C,
                            // so force `->` regardless of source `.`/`->`.
                            return AstExpr {
                                kind: AstExprKind::PropRef, expr_type: None, line, col,
                                data: AstExprData::PropRef { prop: Some(property.to_string()), cls: Some(cls_sym.name.clone()), obj: Box::new(self.convert_expr(object).unwrap_or_else(make_self_expr)), name: property.to_string(), is_arrow: true },
                            };
                        }
                    }
                }
            }
        }
        let obj_ast = self.convert_expr(object).unwrap_or_else(make_self_expr);
        AstExpr {
            kind: AstExprKind::PropRef, expr_type: None, line, col,
            data: AstExprData::PropRef { prop: None, cls: None, obj: Box::new(obj_ast), name: property.to_string(), is_arrow },
        }
    }

    fn convert_ident_expr(&mut self, line: usize, col: usize, name: &str) -> Option<AstExpr> {
        // Check if this identifier is an ivar (like C's convert_expr lines 167-196)
        if let Some(ivar_ref) = self.convert_ivar_ref(line, col, name) {
            return Some(ivar_ref);
        }
        // Resolve from symbol table
        let sym = self.symtab.as_ref().and_then(|st| st.lookup(name));
        Some(AstExpr {
            kind: AstExprKind::VarRef, expr_type: None, line, col,
            data: AstExprData::VarRef { sym: sym.map(|s| s.name.clone()), name: name.to_string() },
        })
    }

    fn convert_stmt(&mut self, cs: &CstStmt) -> Option<AstStmt> {
        let line = cs.line; let col = cs.column;
        let as_ = match &cs.data {
            CstStmtData::Expr(ce) => AstStmt { kind: AstStmtKind::Expr, line, col, data: AstStmtData::Expr(self.convert_expr(ce).unwrap_or_else(make_int_expr)) },
            CstStmtData::Compound(stmts) => {
                let mut ast_stmts = Vec::new();
                for s in stmts { if let Some(a) = self.convert_stmt(s) { ast_stmts.push(a); } }
                AstStmt { kind: AstStmtKind::Compound, line, col, data: AstStmtData::Compound(ast_stmts) }
            }
            CstStmtData::If { cond, then_branch, else_branch } => {
                AstStmt { kind: AstStmtKind::If, line, col, data: AstStmtData::If { cond: Box::new(self.convert_expr(cond).unwrap_or_else(make_int_expr)), then: Box::new(self.convert_stmt(then_branch).unwrap_or_else(make_compound_stmt)), else_: else_branch.as_ref().map(|e| Box::new(self.convert_stmt(e).unwrap_or_else(make_compound_stmt))) } }
            }
            CstStmtData::While { cond, body } => {
                AstStmt { kind: AstStmtKind::While, line, col, data: AstStmtData::While { cond: Box::new(self.convert_expr(cond).unwrap_or_else(make_int_expr)), body: Box::new(self.convert_stmt(body).unwrap_or_else(make_compound_stmt)) } }
            }
            CstStmtData::For { init, cond, incr, body } => {
                AstStmt { kind: AstStmtKind::For, line, col, data: AstStmtData::For { init: init.as_ref().map(|i| Box::new(self.convert_stmt(i).unwrap_or_else(make_compound_stmt))), cond: cond.as_ref().map(|c| Box::new(self.convert_expr(c).unwrap_or_else(make_int_expr))), incr: incr.as_ref().map(|i| Box::new(self.convert_expr(i).unwrap_or_else(make_int_expr))), body: Box::new(self.convert_stmt(body).unwrap_or_else(make_compound_stmt)) } }
            }
            CstStmtData::ForIn { var, collection, body } => {
                AstStmt { kind: AstStmtKind::ForIn, line, col, data: AstStmtData::ForIn { var: Box::new(self.convert_expr(var).unwrap_or_else(make_int_expr)), collection: Box::new(self.convert_expr(collection).unwrap_or_else(make_int_expr)), body: Box::new(self.convert_stmt(body).unwrap_or_else(make_compound_stmt)) } }
            }
            CstStmtData::Do { body, cond } => {
                AstStmt { kind: AstStmtKind::Do, line, col, data: AstStmtData::Do { body: Box::new(self.convert_stmt(body).unwrap_or_else(make_compound_stmt)), cond: Box::new(self.convert_expr(cond).unwrap_or_else(make_int_expr)) } }
            }
            CstStmtData::Switch { expr, body } => {
                AstStmt { kind: AstStmtKind::Switch, line, col, data: AstStmtData::Switch { expr: Box::new(self.convert_expr(expr).unwrap_or_else(make_int_expr)), body: Box::new(self.convert_stmt(body).unwrap_or_else(make_compound_stmt)) } }
            }
            CstStmtData::Case { value, body } => {
                AstStmt { kind: AstStmtKind::Case, line, col, data: AstStmtData::Case { value: Box::new(self.convert_expr(value).unwrap_or_else(make_int_expr)), body: Box::new(self.convert_stmt(body).unwrap_or_else(make_compound_stmt)) } }
            }
            CstStmtData::Default(body) => {
                AstStmt { kind: AstStmtKind::Default, line, col, data: AstStmtData::Default(Box::new(self.convert_stmt(body).unwrap_or_else(make_compound_stmt))) }
            }
            CstStmtData::Return(ce) => {
                let ast_kind = match cs.kind {
                    CstStmtKind::Break => AstStmtKind::Break,
                    CstStmtKind::Continue => AstStmtKind::Continue,
                    _ => AstStmtKind::Return,
                };
                AstStmt { kind: ast_kind, line, col, data: AstStmtData::Return(ce.as_ref().map(|e| Box::new(self.convert_expr(e).unwrap_or_else(make_int_expr)))) }
            }
            CstStmtData::Goto(label) => AstStmt { kind: AstStmtKind::Goto, line, col, data: AstStmtData::Goto(label.clone()) },
            CstStmtData::Label(label) => AstStmt { kind: AstStmtKind::Label, line, col, data: AstStmtData::Label(label.clone()) },
            CstStmtData::Throw(ce) => AstStmt { kind: AstStmtKind::Throw, line, col, data: AstStmtData::Throw(ce.as_ref().map(|e| Box::new(self.convert_expr(e).unwrap_or_else(make_int_expr)))) },
            CstStmtData::Try { try_block, catches, finally_block } => {
                AstStmt { kind: AstStmtKind::Try, line, col, data: AstStmtData::Try { try_block: Box::new(self.convert_stmt(try_block).unwrap_or_else(make_compound_stmt)), catches: catches.iter().filter_map(|c| self.convert_stmt(c)).collect(), finally_block: finally_block.as_ref().map(|f| Box::new(self.convert_stmt(f).unwrap_or_else(make_compound_stmt))) } }
            }
            CstStmtData::Catch { param, body } => {
                AstStmt { kind: AstStmtKind::Catch, line, col, data: AstStmtData::Catch { param: param.clone(), body: Box::new(self.convert_stmt(body).unwrap_or_else(make_compound_stmt)) } }
            }
            CstStmtData::Finally(body) => {
                AstStmt { kind: AstStmtKind::Finally, line, col, data: AstStmtData::Finally(Box::new(self.convert_stmt(body).unwrap_or_else(make_compound_stmt))) }
            }
            CstStmtData::Synchronized { lock, body } => {
                AstStmt { kind: AstStmtKind::Synchronized, line, col, data: AstStmtData::Synchronized { lock: Box::new(self.convert_expr(lock).unwrap_or_else(make_int_expr)), body: Box::new(self.convert_stmt(body).unwrap_or_else(make_compound_stmt)) } }
            }
            CstStmtData::Autoreleasepool(body) => {
                AstStmt { kind: AstStmtKind::Autoreleasepool, line, col, data: AstStmtData::Autoreleasepool(Box::new(self.convert_stmt(body).unwrap_or_else(make_compound_stmt))) }
            }
            CstStmtData::Decl(cd) => {
                if let Some(ad) = self.convert_decl(cd) {
                    AstStmt { kind: AstStmtKind::Decl, line, col, data: AstStmtData::Decl(ad) }
                } else {
                    return None;
                }
            }
        };
        Some(as_)
    }

    fn convert_decl(&mut self, cd: &CstDecl) -> Option<AstDecl> {
        self.convert_decl_with_type(cd, None)
    }

    fn convert_decl_with_type(&mut self, cd: &CstDecl, override_type: Option<Box<CstType>>) -> Option<AstDecl> {
        let line = cd.line; let col = cd.column;
        let ad = match &cd.data {
            CstDeclData::Variable { var_type, initializer, is_static, is_extern, is_const, is_block_qual, is_weak, .. } => {
                let effective_type = override_type.or_else(|| var_type.clone());
                let mut base_ad = AstDecl { kind: AstDeclKind::Variable, line, col, name: cd.name.clone(), data: AstDeclData::Variable { var_type: effective_type.as_ref().and_then(|t| self.convert_type(t)).map(Box::new), init: initializer.as_ref().map(|i| Box::new(self.convert_expr(i).unwrap_or_else(make_int_expr))), is_static: *is_static, is_extern: *is_extern, is_const: *is_const, is_block_qual: *is_block_qual, is_weak: *is_weak, next: None } };
                // Build the next chain (comma-separated declarators) from cd.next
                let base_type = effective_type.clone();
                let mut chain_names: Vec<String> = Vec::new();
                let mut chain_inits: Vec<Option<Box<CstExpr>>> = Vec::new();
                {
                    let mut cur = cd.next.as_ref();
                    while let Some(ncd) = cur {
                        chain_names.push(ncd.name.clone().unwrap_or_default());
                        chain_inits.push(match &ncd.data {
                            CstDeclData::Variable { initializer, .. } => initializer.clone(),
                            _ => None,
                        });
                        cur = ncd.next.as_ref();
                    }
                }
                // Build the AstDecl chain by linking in reverse (last → first, then link to base)
                let mut chain_head: Option<Box<AstDecl>> = None;
                for i in (0..chain_names.len()).rev() {
                    let next_type = base_type.clone();
                    let n_init = chain_inits[i].as_ref().map(|i| Box::new(self.convert_expr(i).unwrap_or_else(make_int_expr)));
                    let ad = AstDecl { kind: AstDeclKind::Variable, line, col, name: Some(chain_names[i].clone()), data: AstDeclData::Variable { var_type: next_type.as_ref().and_then(|t| self.convert_type(t)).map(Box::new), init: n_init, is_static: *is_static, is_extern: *is_extern, is_const: *is_const, is_block_qual: *is_block_qual, is_weak: *is_weak, next: chain_head.take() } };
                    chain_head = Some(Box::new(ad));
                }
                if let AstDeclData::Variable { ref mut next, .. } = base_ad.data {
                    *next = chain_head;
                }
                base_ad
            }
            CstDeclData::Function { return_type, params, body, .. } => {
                let func_sym = cd.name.as_ref().and_then(|n| self.symtab.as_ref()?.lookup(n)).map(|s| s.name.clone());
                AstDecl { kind: AstDeclKind::Function, line, col, name: cd.name.clone(), data: AstDeclData::Function { func_sym, return_type: return_type.as_ref().and_then(|t| self.convert_type(t)).map(Box::new), params: params.clone(), body: body.as_ref().and_then(|b| self.convert_stmt(b)).map(Box::new) } }
            }
            CstDeclData::Class { superclass, ivars, properties, methods, impl_vars, .. } => {
                let fqn = self.ns_fqn(cd.name.as_deref().unwrap_or(""));
                let cls_sym = self.symtab.as_ref().and_then(|st| st.find_class(&fqn)).map(|s| s.name.clone());
                let cls_sym_clone = cls_sym.clone();
                self.current_class_sym = cls_sym.clone();
                // Inject implicit root class __nupa_root for classes without explicit superclass
                let effective_superclass = superclass.as_ref().map(|s| s.clone()).or_else(|| {
                    if fqn != "__nupa_root" { Some("__nupa_root".to_string()) } else { None }
                });
                let sup_name = effective_superclass.as_ref().and_then(|s| {
                    self.symtab.as_ref().and_then(|st| {
                        st.find_class(s).map(|sym| sym.name.clone())
                            .or_else(|| {
                                let fqn = self.ns_fqn(s);
                                st.find_class(&fqn).map(|sym| sym.name.clone())
                            })
                    }).or_else(|| superclass.clone())
                });
                let mut ad = AstDecl { kind: AstDeclKind::Class, line, col, name: Some(self.ns_fqn(cd.name.as_deref().unwrap_or(""))), data: AstDeclData::Class { cls_sym: cls_sym_clone, super_name: sup_name, methods: methods.iter().filter_map(|m| self.convert_decl(m)).collect(), ivars: ivars.iter().filter_map(|iv| self.convert_decl(iv)).collect(), properties: properties.iter().filter_map(|p| self.convert_decl(p)).collect(), impl_vars: impl_vars.iter().filter_map(|v| self.convert_decl(v)).collect() } };
                if let AstDeclData::Class { ref mut methods, .. } = ad.data {
                    if let Some(ref st) = self.symtab {
                        if let Some(ref cls_name) = cls_sym {
                            if let Some(csym) = st.find_class(cls_name) {
                                if let SymbolData::Class { methods: ref cls_method_names, .. } = csym.data {
                                    for m in methods.iter_mut() {
                                        if let AstDeclData::Method { ref mut method_sym, .. } = m.data {
                                            if let Some(ref mname) = m.name {
                                                for cm in cls_method_names {
                                                    if cm == mname {
                                                        *method_sym = Some(cm.clone());
                                                        break;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                self.current_class_sym = None;
                ad
            }
            CstDeclData::Method { is_class_method, return_type, params, body } => {
                // Resolve param type names to FQN for types known in the current namespace
                let mut resolved_params = params.clone();
                if !self.ns_prefix.is_empty() {
                    let mut p = resolved_params.as_mut().map(|b| &mut **b);
                    while let Some(param) = p {
                        if let Some(ref mut pt) = param.par_type {
                            Self::resolve_cst_type_name(pt, &self.ns_prefix, &self.symtab);
                        }
                        p = param.next.as_mut().map(|n| &mut **n);
                    }
                }
                AstDecl { kind: AstDeclKind::Method, line, col, name: cd.name.clone(), data: AstDeclData::Method { method_sym: None, is_class_method: *is_class_method, return_type: return_type.as_ref().and_then(|t| self.convert_type(t)).map(Box::new), params: resolved_params, body: body.as_ref().and_then(|b| self.convert_stmt(b)).map(Box::new) } }
            }
            CstDeclData::Ivar { ivar_type, .. } => {
                let ivar_sym = cd.name.as_ref().and_then(|n| self.symtab.as_ref()?.lookup(n)).map(|s| s.name.clone());
                AstDecl { kind: AstDeclKind::Ivar, line, col, name: cd.name.clone(), data: AstDeclData::Ivar { ivar_sym, ivar_type: ivar_type.as_ref().and_then(|t| self.convert_type(t)).map(Box::new) } }
            }
            CstDeclData::Property { prop_type, getter, setter, is_readonly, is_weak, is_assign, is_retain, is_copy, is_nonatomic, is_dynamic, .. } => {
                let prop_sym = cd.name.as_ref().and_then(|n| self.symtab.as_ref()?.lookup(n)).map(|s| s.name.clone());
                AstDecl { kind: AstDeclKind::Property, line, col, name: cd.name.clone(), data: AstDeclData::Property { prop_sym, prop_type: prop_type.as_ref().and_then(|t| self.convert_type(t)).map(Box::new), getter: getter.clone(), setter: setter.clone(), is_readonly: *is_readonly, is_weak: *is_weak, is_assign: *is_assign, is_retain: *is_retain, is_copy: *is_copy, is_nonatomic: *is_nonatomic, is_dynamic: *is_dynamic } }
            }
            CstDeclData::Typedef { alias_type, struct_fields } => {
                // Store the fully-qualified name so codegen emits the flat name,
                // making the typedef visible from outside the namespace.
                let ty_fqn = if let Some(ref n) = cd.name {
                    if !n.contains("::") && !self.ns_prefix.is_empty() {
                        format!("{}{}", self.ns_prefix, n)
                    } else {
                        n.clone()
                    }
                } else {
                    String::new()
                };
                if let Some(ref mut st) = self.symtab {
                    if !ty_fqn.is_empty() {
                        st.declare(Symbol::new(SymbolKind::Type, &ty_fqn));
                    }
                }
                AstDecl { kind: AstDeclKind::Typedef, line, col, name: Some(ty_fqn.clone()).filter(|s| !s.is_empty()).or_else(|| cd.name.clone()), data: AstDeclData::Typedef { aliased_type: alias_type.as_ref().and_then(|t| self.convert_type(t)).map(Box::new), struct_fields: struct_fields.iter().filter_map(|f| self.convert_decl(f)).collect() } }
            }
            CstDeclData::Aggregate { fields, .. } => {
                AstDecl { kind: AstDeclKind::Struct, line, col, name: cd.name.clone(), data: AstDeclData::Aggregate { fields: fields.iter().filter_map(|f| self.convert_decl(f)).collect() } }
            }
            CstDeclData::Enum { members, values } => {
                AstDecl { kind: AstDeclKind::Enum, line, col, name: cd.name.clone(), data: AstDeclData::Enum { members: members.clone(), values: values.iter().filter_map(|v| self.convert_expr(v)).collect() } }
            }
            CstDeclData::Namespace(decls) => {
                let mut ast_decls = Vec::new();
                for d in decls { self.flatten_decls(d, &mut ast_decls); }
                if ast_decls.len() == 1 { return Some(ast_decls.into_iter().next().unwrap()); }
                else if ast_decls.is_empty() { return None; }
                AstDecl { kind: AstDeclKind::Namespace, line, col, name: None, data: AstDeclData::Namespace(ast_decls) }
            }
            CstDeclData::Forward(_) => return None,
            CstDeclData::ProtocolData { .. } => return None,
            CstDeclData::Using { .. } => {
                // @using is handled by the binder; no AST decl needed
                return None;
            }
        };
        Some(ad)
    }

    fn flatten_decls(&mut self, cd: &CstDecl, out: &mut Vec<AstDecl>) {
        match &cd.data {
            CstDeclData::Namespace(decls) => {
                if let Some(ref name) = cd.name {
                    let old = self.ns_prefix.clone();
                    self.ns_prefix = format!("{}{}::", old, name);
                    for d in decls { self.flatten_decls(d, out); }
                    self.ns_prefix = old;
                } else {
                    for d in decls { self.flatten_decls(d, out); }
                }
                return;
            }
            CstDeclData::Using { .. } => {
                // @using is handled by the binder; no ns_prefix change or AST decl needed
                return;
            }
            _ => {}
        }
        if let Some(ad) = self.convert_decl(cd) {
            out.push(ad);
        }
    }

    pub fn run(&mut self, unit: &TranslationUnit) -> i32 {
        // Step 1: @property elaboration on symbol table (like C's elaborator_run lines 1001-1008)
        if let Some(ref mut st) = self.symtab {
            let class_names: Vec<String> = st.global.symbols.iter()
                .filter(|s| s.kind == SymbolKind::Class)
                .map(|s| s.name.clone())
                .collect();
            for cls_name in class_names {
                Self::elaborate_class(st, &cls_name);
            }
        }

        // Step 2: CST → AST conversion with namespace flattening
        let mut ast_decls = Vec::new();
        for decl in &unit.decls {
            self.flatten_decls(decl, &mut ast_decls);
        }
        self.result = Some(AstUnit { decls: ast_decls, filename: unit.filename.clone() });
        if self.has_error { -1 } else { 0 }
    }

    fn elaborate_class(st: &mut SymbolTable, cls_name: &str) {
        let cls = st.find_class(cls_name);
        let cls = match cls { Some(c) => c.clone(), None => return };
        let prop_names: Vec<String> = match &cls.data {
            SymbolData::Class { ref properties, .. } => properties.clone(),
            _ => return,
        };
        let cls_name = cls.name.clone();
        for prop_name in &prop_names {
            // Find the property symbol in global scope
            let prop_sym = st.global.symbols.iter().find(|s| s.name == *prop_name && s.kind == SymbolKind::Property);
            eprintln!("DBG elaborate_class {:?} prop_name={:?} found_sym={:?}", cls_name, prop_name, prop_sym.is_some());
            let prop_sym = match prop_sym { Some(s) => s.clone(), None => continue };
            let prop_type = match &prop_sym.data {
                SymbolData::Property { prop_type, is_dynamic, .. } => {
                    if *is_dynamic { continue; }
                    prop_type.clone()
                }
                _ => continue,
            };
            // Synthesize ivar: _propName
            let ivar_name = format!("_{}", prop_name);
            {
                let cls = st.find_class(&cls_name);
                if let Some(c) = cls {
                    if let SymbolData::Class { ref ivars, .. } = c.data {
                        if ivars.contains(&ivar_name) { continue; }
                    }
                }
            }
            let ivar_sym = Symbol::new(SymbolKind::Ivar, &ivar_name);
            st.declare(ivar_sym);
            // Add ivar name to class's ivar list
            for sym in st.global.symbols.iter_mut() {
                if sym.name == cls_name && sym.kind == SymbolKind::Class {
                    if let SymbolData::Class { ref mut ivars, .. } = sym.data {
                        ivars.push(ivar_name);
                    }
                    break;
                }
            }
            // Synthesize getter: propName (returns property type)
            let getter_name = prop_name.clone();
            let mut getter_sym = Symbol::new(SymbolKind::Method, &getter_name);
            getter_sym.data = SymbolData::Method {
                is_class_method: false,
                return_type: prop_type.clone(),
                params: None,
                has_body: true,
                vtable_index: -1,
                owner_class: Some(cls_name.clone()),
            };
            st.declare(getter_sym);
            for sym in st.global.symbols.iter_mut() {
                if sym.name == cls_name && sym.kind == SymbolKind::Class {
                    if let SymbolData::Class { ref mut methods, .. } = sym.data {
                        if !methods.contains(&getter_name) {
                            methods.push(getter_name.clone());
                        }
                    }
                    break;
                }
            }
            // Synthesize setter: setPropName: (void return, takes property type)
            if !matches!(prop_sym.data, SymbolData::Property { is_readonly: true, .. }) {
                let setter_name = format!("set{}{}:", 
                    prop_name[0..1].to_uppercase(),
                    &prop_name[1..]);
                let mut setter_sym = Symbol::new(SymbolKind::Method, &setter_name);
                let void_type = NpType::new(TypePrim::Void);
                let param = NpParam {
                    par_type: prop_type.clone(),
                    name: Some("value".to_string()),
                    next: None,
                };
                setter_sym.data = SymbolData::Method {
                    is_class_method: false,
                    return_type: Some(Box::new(void_type)),
                    params: Some(Box::new(param)),
                    has_body: true,
                    vtable_index: -1,
                    owner_class: Some(cls_name.clone()),
                };
                st.declare(setter_sym);
                for sym in st.global.symbols.iter_mut() {
                    if sym.name == cls_name && sym.kind == SymbolKind::Class {
                        if let SymbolData::Class { ref mut methods, .. } = sym.data {
                            if !methods.contains(&setter_name) {
                                methods.push(setter_name);
                            }
                        }
                        break;
                    }
                }
            }
        }
    }

    pub fn take_ast(&mut self) -> Option<AstUnit> {
        self.result.take()
    }
}

fn make_int_expr() -> AstExpr {
    AstExpr { kind: AstExprKind::Int, expr_type: None, line: 0, col: 0, data: AstExprData::Int(0) }
}

fn make_self_expr() -> AstExpr {
    AstExpr { kind: AstExprKind::Self_, expr_type: None, line: 0, col: 0, data: AstExprData::VarRef { sym: None, name: "self".into() } }
}

fn make_compound_stmt() -> AstStmt {
    AstStmt { kind: AstStmtKind::Compound, line: 0, col: 0, data: AstStmtData::Compound(Vec::new()) }
}