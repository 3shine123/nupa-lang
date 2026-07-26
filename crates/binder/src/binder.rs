use nupa_cst::*;
use nupa_symbol::symbol::*;

// Strip trailing `*` from a type expression, return (stripped, ptr_level)
fn strip_ptr(fqn: &str) -> (String, usize) {
    let trimmed = fqn.trim();
    let mut ptr_level = 0;
    let mut s = trimmed;
    while s.ends_with('*') {
        ptr_level += 1;
        s = s[..s.len()-1].trim();
    }
    (s.to_string(), ptr_level)
}

// Extract the base class name from a type expression (remove `<...>`)
fn base_class_name(fqn: &str) -> String {
    let s = fqn.trim();
    if let Some(pos) = s.find('<') {
        s[..pos].trim().to_string()
    } else {
        s.to_string()
    }
}

pub struct Binder {
    pub symtab: SymbolTable,
    pub current_class: Option<String>,
    pub has_error: bool,
    pub err_msg: String,
    pub ns_prefix: String,
}

impl Binder {
    pub fn new(symtab: SymbolTable) -> Self {
        let mut binder = Binder { symtab, current_class: None, has_error: false, err_msg: String::new(), ns_prefix: String::new() };
        // Register built-in implicit root class __nupa_root
        binder.symtab.declare(Symbol::new(SymbolKind::Class, "__nupa_root"));
        binder
    }

    pub fn has_error(&self) -> bool { self.has_error }
    pub fn last_error(&self) -> &str { &self.err_msg }

    fn error(&mut self, line: usize, col: usize, msg: &str) {
        self.has_error = true;
        self.err_msg = msg.to_string();
        eprintln!("error:{}:{}: {}", line, col, msg);
    }

    fn ns_fqn(&self, name: &str) -> String {
        if name.contains("::") { return name.to_string(); }
        // Check @using namespace entries — try <ns>::<name>
        for entry in &self.symtab.using_list {
            if entry.is_namespace {
                let prefixed = format!("{}::{}", entry.fqn, name);
                if self.symtab.find_class(&prefixed).is_some() || self.symtab.find_protocol(&prefixed).is_some() {
                    return prefixed;
                }
            }
        }
        if self.ns_prefix.is_empty() { name.to_string() }
        else { format!("{}{}", self.ns_prefix, name) }
    }

    fn build_selector_name(&self, d: &CstDecl) -> String {
        if let CstDeclData::Method { ref params, .. } = d.data {
            if let Some(ref p) = params {
                let mut sel = String::new();
                let mut cur = Some(p.as_ref());
                while let Some(param) = cur {
                    if let Some(ref ext) = param.external_name {
                        sel.push_str(ext);
                    }
                    cur = param.next.as_ref().map(|n| n.as_ref());
                }
                if !sel.is_empty() { return sel; }
            }
        }
        d.name.clone().unwrap_or_default()
    }

    fn params_to_method_sym(&self, d: &CstDecl, owner: &str) -> Symbol {
        let sel = self.build_selector_name(d);
        let mut msym = Symbol::new(SymbolKind::Method, &sel);
        if let CstDeclData::Method { is_class_method, ref return_type, ref params, ref body } = d.data {
            msym = Symbol::new(SymbolKind::Method, &sel);
            msym.data = SymbolData::Method {
                is_class_method,
                return_type: return_type.as_ref().map(|rt| Box::new(NpType::from_cst(rt))),
                params: params.as_ref().map(|p| {
                    let mut head = Box::new(NpParam {
                        par_type: p.par_type.as_ref().map(|pt| Box::new(NpType::from_cst(pt))),
                        name: p.name.clone(),
                        next: None,
                    });
                    let mut tail = &mut head;
                    let mut cur = p.next.as_ref().map(|n| n.as_ref());
                    while let Some(cp) = cur {
                        let np = Box::new(NpParam {
                            par_type: cp.par_type.as_ref().map(|pt| Box::new(NpType::from_cst(pt))),
                            name: cp.name.clone(),
                            next: None,
                        });
                        tail.next = Some(np);
                        tail = tail.next.as_mut().unwrap();
                        cur = cp.next.as_ref().map(|n| n.as_ref());
                    }
                    head
                }),
                has_body: body.is_some(),
                vtable_index: -1,
                owner_class: Some(owner.to_string()),
            };
        }
        msym
    }

    fn bind_type(&mut self, ct: &mut CstType) {
        if ct.prim == TypePrim::Named {
            if let Some(ref name) = ct.name.clone() {
                let cls = self.symtab.find_class(name)
                    .or_else(|| {
                        let fqn = self.ns_fqn(name);
                        self.symtab.find_class(&fqn)
                    });
                if let Some(c) = cls {
                    if c.name != *name {
                        let new_name = Some(c.name.clone());
                        ct.name = new_name.clone();
                        if let Some(ref mut sub) = ct.subtype {
                            if sub.name == Some(name.clone()) {
                                sub.name = new_name;
                            }
                        }
                    }
                    return;
                }
                if self.symtab.find_protocol(name).is_some() { return; }
                let type_sym = self.symtab.lookup(name)
                    .and_then(|s| if s.kind == SymbolKind::Type { Some(s) } else { None });
                if let Some(s) = type_sym {
                    if s.name != *name { ct.name = Some(s.name.clone()); }
                    return;
                }
            }
        }
        if let Some(ref mut sub) = ct.subtype { self.bind_type(sub); }
        if let Some(ref mut n) = ct.next { self.bind_type(n); }
        for arg in &mut ct.type_args { self.bind_type(arg); }
        if let Some(ref mut bp) = ct.block_params { self.bind_type(bp); }
    }

    fn bind_type_opt(&mut self, ct: &mut Option<Box<CstType>>) {
        if let Some(ref mut t) = ct { self.bind_type(t); }
    }

    fn bind_params(&mut self, params: &mut Option<Box<CstParam>>) {
        if let Some(ref mut param) = params {
            self.bind_type_opt(&mut param.par_type);
            self.bind_params(&mut param.next);
        }
    }

    fn bind_expr(&mut self, e: &mut CstExpr) {
        match &mut e.data {
            CstExprData::Ident(name) => {
                if self.symtab.lookup(name).is_none() {
                    if let Some(ref cls_name) = self.current_class {
                        if let Some(cls_sym) = self.symtab.find_class(cls_name) {
                            if let SymbolData::Class { ref ivars, .. } = cls_sym.data {
                                if ivars.contains(name) { return; }
                            }
                        }
                    }
                    self.symtab.declare(Symbol::new(SymbolKind::Variable, name));
                }
            }
            CstExprData::Message { receiver, args, .. } => {
                self.bind_expr(receiver);
                for a in args.iter_mut() { self.bind_expr(a); }
            }
            CstExprData::Dot { object, .. } => { self.bind_expr(object); }
            CstExprData::Arrow { object, .. } => { self.bind_expr(object); }
            CstExprData::Subscript { object, key } => {
                self.bind_expr(object);
                self.bind_expr(key);
            }
            CstExprData::Call { callee, args } => {
                self.bind_expr(callee);
                for a in args.iter_mut() { self.bind_expr(a); }
            }
            CstExprData::Unary { operand, .. } => { self.bind_expr(operand); }
            CstExprData::Binary { left, right, .. } => {
                self.bind_expr(left);
                self.bind_expr(right);
            }
            CstExprData::Ternary { cond, true_expr, false_expr } => {
                self.bind_expr(cond);
                self.bind_expr(true_expr);
                self.bind_expr(false_expr);
            }
            CstExprData::Assign { target, value } => {
                self.bind_expr(target);
                self.bind_expr(value);
            }
            CstExprData::Cast { ref mut target_type, expr } => {
                self.bind_type(target_type);
                self.bind_expr(expr);
            }
            CstExprData::Sizeof { ref mut type_expr, expr } => {
                self.bind_type(type_expr);
                if let Some(ref mut ex) = expr { self.bind_expr(ex); }
            }
            CstExprData::Typeof(ref mut ct) => { self.bind_type(ct); }
            CstExprData::Comma(exprs) => { for ex in exprs.iter_mut() { self.bind_expr(ex); } }
            CstExprData::Block { ref mut params, return_type: _, ref mut body, .. } => {
                let mut cur = params.as_mut().map(|p| p.as_mut());
                while let Some(param) = cur {
                    self.bind_type_opt(&mut param.par_type);
                    if let Some(ref pname) = param.name.clone() {
                        self.symtab.declare(Symbol::new(SymbolKind::Variable, &pname));
                    }
                    cur = param.next.as_mut().map(|n| n.as_mut());
                }
                if let Some(ref mut b) = body { self.bind_stmt(b); }
            }
            CstExprData::ArrayLit(elements) => { for ex in elements.iter_mut() { self.bind_expr(ex); } }
            CstExprData::DictLit { ref mut keys, ref mut values } => {
                for k in keys.iter_mut() { self.bind_expr(k); }
                for v in values.iter_mut() { self.bind_expr(v); }
            }
            CstExprData::NumberLit(val) => { self.bind_expr(val); }
            CstExprData::Selector(ref sel_name) => {
                self.symtab.register_selector(sel_name);
            }
            CstExprData::Encode(ref mut ct) => { self.bind_type(ct); }
            CstExprData::InitList(exprs) => { for ex in exprs.iter_mut() { self.bind_expr(ex); } }
            CstExprData::Paren(inner) => { self.bind_expr(inner); }
            _ => {}
        }
    }

    fn bind_stmt(&mut self, s: &mut CstStmt) {
        match &mut s.data {
            CstStmtData::Expr(e) => { self.bind_expr(e); }
            CstStmtData::Compound(stmts) => {
                for stmt in stmts.iter_mut() { self.bind_stmt(stmt); }
            }
            CstStmtData::If { cond, then_branch, else_branch } => {
                self.bind_expr(cond);
                self.bind_stmt(then_branch);
                if let Some(ref mut eb) = else_branch { self.bind_stmt(eb); }
            }
            CstStmtData::Switch { expr, body } => {
                self.bind_expr(expr);
                self.bind_stmt(body);
            }
            CstStmtData::Case { value, body } => {
                self.bind_expr(value);
                self.bind_stmt(body);
            }
            CstStmtData::Default(body) => { self.bind_stmt(body); }
            CstStmtData::While { cond, body } => {
                self.bind_expr(cond);
                self.bind_stmt(body);
            }
            CstStmtData::Do { body, cond } => {
                self.bind_stmt(body);
                self.bind_expr(cond);
            }
            CstStmtData::For { init, cond, incr, body } => {
                if let Some(ref mut i) = init { self.bind_stmt(i); }
                if let Some(ref mut c) = cond { self.bind_expr(c); }
                if let Some(ref mut i) = incr { self.bind_expr(i); }
                self.bind_stmt(body);
            }
            CstStmtData::ForIn { var, collection, body } => {
                self.bind_expr(var);
                self.bind_expr(collection);
                self.bind_stmt(body);
            }
            CstStmtData::Return(val) => { if let Some(ref mut v) = val { self.bind_expr(v); } }
            CstStmtData::Throw(val) => { if let Some(ref mut v) = val { self.bind_expr(v); } }
            CstStmtData::Try { try_block, catches, finally_block } => {
                self.bind_stmt(try_block);
                for c in catches.iter_mut() { self.bind_stmt(c); }
                if let Some(ref mut f) = finally_block { self.bind_stmt(f); }
            }
            CstStmtData::Catch { ref mut param, body } => {
                self.bind_type_opt(&mut param.par_type);
                if let Some(ref pname) = param.name.clone() {
                    self.symtab.declare(Symbol::new(SymbolKind::Variable, &pname));
                }
                self.bind_stmt(body);
            }
            CstStmtData::Finally(body) => { self.bind_stmt(body); }
            CstStmtData::Synchronized { lock, body } => {
                self.bind_expr(lock);
                self.bind_stmt(body);
            }
            CstStmtData::Autoreleasepool(body) => { self.bind_stmt(body); }
            CstStmtData::Decl(ref mut d) => { self.bind_decl(d); }
            _ => {}
        }
    }

    fn bind_method_body(&mut self, d: &mut CstDecl) {
        if let CstDeclData::Method { ref mut body, ref params, .. } = d.data {
            if let Some(ref mut b) = body {
                let mut cur = params.as_ref().map(|p| p.as_ref());
                while let Some(p) = cur {
                    if let Some(ref pname) = p.name {
                        self.symtab.declare(Symbol::new(SymbolKind::Variable, pname));
                    }
                    cur = p.next.as_ref().map(|n| n.as_ref());
                }
                self.bind_stmt(b);
            }
        }
    }

    fn bind_decl(&mut self, d: &mut CstDecl) {
        let kind = d.kind;
        match kind {
            CstDeclKind::ClassInterface => {
                let cls_name = self.ns_fqn(d.name.as_deref().unwrap_or(""));
                let old_class = self.current_class.clone();

                let is_category = matches!(d.data, CstDeclData::Class { ref category_name, .. } if category_name.is_some());
                // Capture superclass from the CST so the elaborator's ivar resolver
                // can walk the superclass chain (subclass methods referencing an
                // ivar declared in the parent, e.g. `_nodeType` declared in
                // NPJsonNode used inside NPJsonStringNode's init).
                let superclass_from_cst = match &d.data {
                    CstDeclData::Class { ref superclass, .. } => superclass.clone(),
                    _ => None,
                };
                if self.symtab.find_class(&cls_name).is_none() {
                    self.symtab.declare(Symbol::new(SymbolKind::Class, &cls_name));
                }
                // Record superclass on the class symbol (overwrite if already set,
                // e.g. a forward @class declaration left it None).
                if let Some(ref sup) = superclass_from_cst {
                    let sup_fqn = if self.symtab.find_class(sup).is_some() {
                        sup.clone()
                    } else {
                        self.ns_fqn(sup)
                    };
                    for sym in self.symtab.global.symbols.iter_mut() {
                        if sym.name == cls_name && sym.kind == SymbolKind::Class {
                            if let SymbolData::Class { ref mut superclass, .. } = sym.data {
                                *superclass = Some(sup_fqn);
                            }
                            break;
                        }
                    }
                }
                self.current_class = Some(cls_name.clone());

                // Collect ivar/property/method names from CST (for class symbol data)
                let mut ivar_names = Vec::new();
                let mut prop_names = Vec::new();
                let mut method_names = Vec::new();

                if !is_category {
                    if let CstDeclData::Class { ref mut ivars, ref mut properties, .. } = d.data {
                        for ivar in ivars.iter_mut() {
                            if let Some(ref n) = ivar.name { ivar_names.push(n.clone()); }
                            self.bind_decl(ivar);
                        }
                        for prop in properties.iter_mut() {
                            if let Some(ref n) = prop.name { prop_names.push(n.clone()); }
                            self.bind_decl(prop);
                            // Follow the next chain (comma-separated properties)
                            let mut cur = prop.next.as_mut().map(|n| n.as_mut());
                            while let Some(next_prop) = cur {
                                if let Some(ref n) = next_prop.name { prop_names.push(n.clone()); }
                                self.bind_decl(next_prop);
                                cur = next_prop.next.as_mut().map(|n| n.as_mut());
                            }
                        }
                    }
                }
                if let CstDeclData::Class { ref mut methods, .. } = d.data {
                    for method in methods.iter_mut() {
                        let sel = self.build_selector_name(method);
                        method_names.push(sel);
                        self.bind_decl(method);
                    }
                }

                // Update class symbol data with ivar/property/method names
                for sym in self.symtab.global.symbols.iter_mut() {
                    if sym.name == cls_name && sym.kind == SymbolKind::Class {
                        if let SymbolData::Class { ref mut ivars, ref mut properties, ref mut methods, .. } = sym.data {
                            ivars.extend(ivar_names);
                            properties.extend(prop_names);
                            methods.extend(method_names);
                        }
                        break;
                    }
                }

                self.current_class = old_class;
            }

            CstDeclKind::ClassImplementation => {
                let cls_name = self.ns_fqn(d.name.as_deref().unwrap_or(""));
                let old_class = self.current_class.clone();
                if self.symtab.find_class(&cls_name).is_none() {
                    self.error(d.line, d.column, &format!("cannot find class '{}' for @implementation", cls_name));
                    return;
                }
                self.current_class = Some(cls_name);

                let mut mcount = 0;
                if let CstDeclData::Class { ref methods, .. } = d.data {
                    mcount = methods.len();
                }
                let mut indices = Vec::new();
                for i in 0..mcount {
                    if let CstDeclData::Class { ref methods, .. } = d.data {
                        if i < methods.len() && methods[i].kind == CstDeclKind::Method {
                            indices.push(i);
                        }
                    }
                }
                for i in indices {
                    if let CstDeclData::Class { ref mut methods, .. } = d.data {
                        if i < methods.len() {
                            self.bind_decl(&mut methods[i]);
                        }
                    }
                }
                // Bind C-level declarations inside @implementation (e.g. static
                // helper functions and variables stored in impl_vars) so that
                // namespace-qualified types (e.g. `Table` → `TOML::Table`) are
                // resolved via ns_prefix.
                if let CstDeclData::Class { ref mut impl_vars, .. } = d.data {
                    for v in impl_vars.iter_mut() {
                        self.bind_decl(v);
                    }
                }
                self.current_class = old_class;
            }

            CstDeclKind::Method => {
                if let CstDeclData::Method { ref mut return_type, ref mut params, .. } = d.data {
                    self.bind_type_opt(return_type);
                    self.bind_params(params);
                }
                self.bind_method_body(d);
            }

            CstDeclKind::Variable => {
                if let CstDeclData::Variable { ref mut var_type, ref mut initializer, is_block_qual, is_weak, .. } = d.data {
                    self.bind_type_opt(var_type);
                    if let Some(ref mut init) = initializer { self.bind_expr(init); }
                    // Register the variable with its type so the elaborator's
                    // convert_dot_expr can resolve `obj.field` access to the
                    // correct ObjC class (forcing `->` for pointer-typed
                    // instances). Without this, `s.grade` on `Student *s`
                    // falls through to plain C `.` and fails to compile.
                    if let Some(ref name) = d.name {
                        let var_t = var_type.as_ref().map(|t| Box::new(NpType::from_cst(t)));
                        if self.symtab.lookup(name).is_none() {
                            let mut sym = Symbol::new(SymbolKind::Variable, name);
                            if let SymbolData::Variable { ref mut var_type, is_static: _, is_extern: _, is_const: _, is_weak: ref mut w, is_block: ref mut b } = sym.data {
                                *var_type = var_t;
                                *w = is_weak;
                                *b = is_block_qual;
                            }
                            self.symtab.declare(sym);
                        } else if let Some(existing) = self.symtab.current.symbols.iter_mut().find(|s| s.name == *name) {
                            // Update existing variable's type if missing
                            if let SymbolData::Variable { ref mut var_type, .. } = existing.data {
                                if var_type.is_none() { *var_type = var_t; }
                            }
                        }
                    }
                }
            }

            CstDeclKind::Typedef => {
                if let CstDeclData::Typedef { ref mut alias_type, .. } = d.data {
                    self.bind_type_opt(alias_type);
                }
                if let Some(ref name) = d.name {
                    if self.symtab.lookup(name).is_none() {
                        self.symtab.declare(Symbol::new(SymbolKind::Type, &self.ns_fqn(name)));
                    }
                }
            }

            CstDeclKind::Struct | CstDeclKind::Union => {
                if let Some(ref name) = d.name {
                    if self.symtab.find_class(name).is_none() && self.symtab.lookup(name).is_none() {
                        self.symtab.declare(Symbol::new(SymbolKind::Class, name));
                    }
                }
                if let CstDeclData::Aggregate { ref mut fields, .. } = d.data {
                    for f in fields.iter_mut() { self.bind_decl(f); }
                }
            }

            CstDeclKind::Enum => {
                if let Some(ref name) = d.name {
                    if self.symtab.lookup(name).is_none() {
                        self.symtab.declare(Symbol::new(SymbolKind::Type, name));
                    }
                }
                if let CstDeclData::Enum { ref members, ref mut values } = d.data {
                    for (i, member) in members.iter().enumerate() {
                        if self.symtab.lookup(member).is_none() {
                            self.symtab.declare(Symbol::new(SymbolKind::Variable, member));
                        }
                        if i < values.len() { self.bind_expr(&mut values[i]); }
                    }
                }
            }

            CstDeclKind::Ivar => {
                if let CstDeclData::Ivar { ref mut ivar_type, .. } = d.data {
                    self.bind_type_opt(ivar_type);
                }
            }

            CstDeclKind::Property => {
                // Snapshot the property data we need by cloning the
                // non-mutable fields. We bind the type separately.
                let (prop_type_clone, getter_c, setter_c, is_readonly, is_weak, is_assign, is_retain, is_copy, is_dynamic, is_nonatomic) =
                    if let CstDeclData::Property { ref prop_type, ref getter, ref setter, ref is_readonly, ref is_weak, ref is_assign, ref is_retain, ref is_copy, ref is_dynamic, ref is_nonatomic, .. } = d.data {
                        (prop_type.clone(), getter.clone(), setter.clone(), *is_readonly, *is_weak, *is_assign, *is_retain, *is_copy, *is_dynamic, *is_nonatomic)
                    } else { (None, None, None, false, false, false, false, false, false, false) };
                // Bind the type in place so namespace qualifiers resolve.
                if let CstDeclData::Property { ref mut prop_type, .. } = d.data {
                    self.bind_type_opt(prop_type);
                }
                // Register the property symbol in the global scope so the
                // elaborator's elaborate_class() can synthesize the backing
                // ivar (e.g. `_name` for `name`) and getter/setter methods.
                // Without this, `@synthesize name = _name` looks up a missing
                // property symbol and the ivar list stays empty, leaving bare
                // `_name` references undeclared in method bodies.
                if let Some(ref name) = d.name {
                    // Register by the original (possibly FQN'd) name, matching
                    // the class symbol's `properties` list which uses the same
                    // name form. elaborate_class() looks up properties by the
                    // same name stored in the class's `properties` field.
                    // Property symbols go directly into the global scope
                    // (declare() only auto-globals Class/Protocol/Type/Function).
                    if self.symtab.global.symbols.iter().find(|s| s.name == *name && s.kind == SymbolKind::Property).is_none() {
                        let mut sym = Symbol::new(SymbolKind::Property, name);
                        sym.data = SymbolData::Property {
                            prop_type: prop_type_clone.as_ref().map(|t| Box::new(NpType::from_cst(t))),
                            ivar_sym: None,
                            getter: getter_c,
                            setter: setter_c,
                            is_readonly,
                            is_weak,
                            is_assign,
                            is_retain,
                            is_copy,
                            is_dynamic,
                            is_nonatomic,
                        };
                        self.symtab.global.add(sym.clone());
                        self.symtab.current.add(sym);
                    }
                }
            }

            CstDeclKind::Function => {
                if let CstDeclData::Function { ref mut return_type, ref mut params, ref mut body, .. } = d.data {
                    self.bind_type_opt(return_type);
                    self.bind_params(params);
                    if let Some(ref mut b) = body { self.bind_stmt(b); }
                }
                if let Some(ref name) = d.name {
                    if self.symtab.lookup(name).is_none() {
                        self.symtab.declare(Symbol::new(SymbolKind::Function, name));
                    }
                }
            }

            CstDeclKind::Protocol => {
                if let Some(ref name) = d.name {
                    if self.symtab.find_protocol(name).is_none() {
                        self.symtab.declare(Symbol::new(SymbolKind::Protocol, name));
                    }
                }
                if let CstDeclData::ProtocolData { ref mut methods, .. } = d.data {
                    for m in methods.iter_mut() { self.bind_decl(m); }
                }
            }

            CstDeclKind::ForwardClass => {
                if let CstDeclData::Forward(ref names) = d.data {
                    for name in names.iter() {
                        let fqn = self.ns_fqn(name);
                        if self.symtab.find_class(&fqn).is_none() {
                            self.symtab.declare(Symbol::new(SymbolKind::Class, &fqn));
                        }
                    }
                }
            }

            CstDeclKind::Namespace => {
                if let CstDeclData::Namespace(ref mut decls) = d.data {
                    if let Some(ref name) = d.name {
                        let old = self.ns_prefix.clone();
                        self.ns_prefix = format!("{}{}::", old, name);
                        for decl in decls.iter_mut() { self.bind_decl(decl); }
                        self.ns_prefix = old;
                    }
                }
            }

            CstDeclKind::Using => {
                if let CstDeclData::Using { ref fqn, ref alias } = d.data {
                    if let Some(ref a) = alias {
                        // @using Alias = TypeExpr*;
                        // Strip trailing `*` and type args for the find_class check,
                        // but store the base FQN (with type args) for later resolution.
                        let (base_fqn, ptr_level) = strip_ptr(fqn);
                        let base_name = base_class_name(&base_fqn);
                        if base_name == "id" || self.symtab.find_class(&base_name).is_some() || self.symtab.find_protocol(&base_name).is_some() {
                            self.symtab.add_using(&base_fqn, a, ptr_level, false);
                        }
                    } else if fqn.contains("::") {
                        let short = fqn.rsplit("::").next().unwrap_or(fqn);
                        if self.symtab.find_class(fqn).is_some() || self.symtab.find_protocol(fqn).is_some() {
                            self.symtab.add_using(fqn, short, 0, false);
                        } else {
                            // @using namespace Engine::Physics; — register as namespace prefix
                            self.symtab.add_using(fqn, fqn, 0, true);
                        }
                    } else {
                        // @using namespace Render; — register as namespace prefix
                        self.symtab.add_using(fqn, fqn, 0, true);
                    }
                }
            }

            _ => {}
        }
    }

    pub fn bind(&mut self, unit: &mut TranslationUnit) -> i32 {
        for decl in unit.decls.iter_mut() { self.bind_decl(decl); }
        if self.has_error { -1 } else { 0 }
    }
}