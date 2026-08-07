use nupa_ast::*;
use nupa_cfg::*;
use nupa_ownership::*;
use nupa_cst::TypePrim;

#[derive(Debug, Clone, Copy)]
pub enum ArcActionKind {
    Retain, Release, Autorelease,
}

#[derive(Debug, Clone)]
pub struct ArcAction {
    pub kind: ArcActionKind,
    pub target: Option<Box<AstExpr>>,
    pub insert_after_idx: usize,
    pub insert_at_end: bool,
}

#[derive(Debug, Clone)]
pub struct ArcResult {
    pub actions: Vec<ArcAction>,
}

impl ArcResult {
    pub fn new() -> Self { ArcResult { actions: Vec::new() } }
}

fn make_release_stmt(target: &AstExpr) -> AstStmt {
    AstStmt {
        kind: AstStmtKind::Expr, line: 0, col: 0,
        data: AstStmtData::Expr(AstExpr {
            kind: AstExprKind::FuncCall, expr_type: None, line: 0, col: 0,
            data: AstExprData::FuncCall {
                func: None, name: "nupa_release".to_string(), callee: None, args: vec![target.clone()],
            },
        }),
    }
}

fn is_object_type(t: &AstType) -> bool {
    t.is_pointer || t.prim == TypePrim::Id || t.prim == TypePrim::Instancetype
}

fn is_return_stmt(s: &AstStmt) -> bool {
    matches!(s.data, AstStmtData::Return(_))
}

fn is_scope_stmt(s: &AstStmt) -> bool {
    matches!(s.data,
        AstStmtData::Compound(_) |
        AstStmtData::Autoreleasepool(_) |
        AstStmtData::Synchronized { .. }
    )
}

fn var_ref_expr(name: &str) -> AstExpr {
    AstExpr {
        kind: AstExprKind::VarRef, expr_type: None, line: 0, col: 0,
        data: AstExprData::VarRef { sym: None, name: name.to_string() },
    }
}

// Insert releases before a statement at index `pos`, skipping any variable that
// is being returned/thrown. `vars` is drained (released vars removed). Returns
// the number of releases inserted.
fn extract_returned_var(e: &AstExpr) -> Option<&str> {
    match &e.data {
        AstExprData::VarRef { name, .. } => Some(name.as_str()),
        AstExprData::MsgSend { receiver, .. } => {
            if let AstExprData::VarRef { name, .. } = &receiver.data {
                Some(name.as_str())
            } else { None }
        }
        _ => None,
    }
}

fn insert_releases_before(stmts: &mut Vec<AstStmt>, pos: usize, vars: &mut Vec<String>, return_expr: Option<&AstExpr>) -> usize {
    let returned_var = return_expr.and_then(|e| extract_returned_var(e));
    let mut count = 0;
    let mut i = 0;
    while i < vars.len() {
        let name = vars[i].clone();
        if returned_var.map_or(false, |rv| rv == name.as_str()) {
            vars.remove(i);
            continue;
        }
        stmts.insert(pos, make_release_stmt(&var_ref_expr(&name)));
        vars.remove(i);
        count += 1;
    }
    count
}

// ─── local analysis (directly inserts releases into AST) ───────────────────

// One scope frame on the analysis stack. `vars` are retained object locals
// declared in this scope; `inside_loop` marks a loop body so that break/continue
// only release variables declared within the loop (not its enclosing scopes).
struct Scope {
    vars: Vec<String>,
    inside_loop: bool,
}

impl Scope {
    fn new(inside_loop: bool) -> Self { Scope { vars: Vec::new(), inside_loop } }
}

// Collect all live vars in scopes `stack[from..]` (deduped, top-level last).
fn collect_vars(stack: &[Scope], from: usize) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for s in stack[from.min(stack.len())..].iter() {
        for n in s.vars.iter() {
            if !v.contains(n) { v.push(n.clone()); }
        }
    }
    v
}

pub fn arc_local_analyze(body: &mut AstStmt, _cfg: &Cfg, method_name: &str) -> ArcResult {
    let _ = ownership_for_method(method_name);
    let mut res = ArcResult::new();

    let stmts = match body.data {
        AstStmtData::Compound(ref mut stmts) => stmts,
        _ => return res,
    };

    fn analyze_scope(stmts: &mut Vec<AstStmt>, res: &mut ArcResult, stack: &mut Vec<Scope>, inside_loop: bool) {
        stack.push(Scope::new(inside_loop));
        let my_idx = stack.len() - 1;

        let mut i = 0;
        while i < stmts.len() {
            // ── Handle nested scopes (recurse) ──
            if is_scope_stmt(&stmts[i]) {
                let inner: *mut Vec<AstStmt> = match &mut stmts[i].data {
                    AstStmtData::Compound(ref mut inner) => inner as *mut Vec<AstStmt>,
                    AstStmtData::Autoreleasepool(ref mut body) => {
                        if let AstStmtData::Compound(ref mut inner) = body.data { inner as *mut Vec<AstStmt> } else { std::ptr::null_mut() }
                    }
                    AstStmtData::Synchronized { ref mut body, .. } => {
                        if let AstStmtData::Compound(ref mut inner) = body.data { inner as *mut Vec<AstStmt> } else { std::ptr::null_mut() }
                    }
                    _ => std::ptr::null_mut(),
                };
                if !inner.is_null() {
                    unsafe { recurse_scope(&mut *inner, res, stack); }
                }
                i += 1;
                continue;
            }

            // ── Handle variable declarations ──
            if let AstStmtData::Decl(d) = &stmts[i].data {
                if let AstDeclData::Variable { init: Some(ref init_val), ref var_type, .. } = d.data {
                    if ownership_for_expr(init_val) == Ownership::Retained {
                        if let Some(ref name) = d.name {
                            if var_type.as_ref().map_or(false, |t| is_object_type(t)) {
                                stack[my_idx].vars.push(name.clone());
                            }
                        }
                    }
                }
                i += 1;
                continue;
            }

            // ── Handle break/continue: release ONLY vars inside the nearest loop ──
            if matches!(stmts[i].kind, AstStmtKind::Break | AstStmtKind::Continue) {
                let mut from = stack.len();
                for idx in (0..stack.len()).rev() {
                    from = idx;
                    if stack[idx].inside_loop { break; }
                }
                let mut to_release = collect_vars(stack, from);
                let n = insert_releases_before(stmts, i, &mut to_release, None);
                for s in stack[from..].iter_mut() { s.vars.clear(); }
                i += 1 + n;
                continue;
            }

            // ── Handle return: release all live vars (current + enclosing), then exit ──
            let is_return = matches!(stmts[i].data, AstStmtData::Return(_));
            if is_return {
                let ret_expr_clone = match &stmts[i].data { AstStmtData::Return(e) => e.clone(), _ => None };
                let mut all = collect_vars(stack, 0);
                let n = insert_releases_before(stmts, i, &mut all, ret_expr_clone.as_ref().map(|e| e.as_ref()));
                clear_all(stack);
                i += 1 + n;
                continue;
            }

            // ── Handle throw: release all live vars, then rethrow ──
            if matches!(stmts[i].data, AstStmtData::Throw(_)) {
                let throw_expr = match &stmts[i].data { AstStmtData::Throw(e) => e.clone(), _ => None };
                let mut all = collect_vars(stack, 0);
                let n = insert_releases_before(stmts, i, &mut all, throw_expr.as_ref().map(|e| e.as_ref()));
                clear_all(stack);
                i += 1 + n;
                continue;
            }

            // ── Handle if/else: recurse into branches ──
            if stmts[i].kind == AstStmtKind::If {
                let if_taken = std::mem::replace(&mut stmts[i].data, AstStmtData::Expr(AstExpr { kind: AstExprKind::Int, expr_type: None, line: 0, col: 0, data: AstExprData::Int(0) }));
                if let AstStmtData::If { cond, mut then, mut else_ } = if_taken {
                    if !matches!(then.data, AstStmtData::Compound(_)) {
                        let body = std::mem::replace(&mut then.data, AstStmtData::Compound(vec![]));
                        then.data = AstStmtData::Compound(vec![AstStmt { kind: then.kind, line: then.line, col: then.col, data: body }]);
                    }
                    if let AstStmtData::Compound(ref mut inner) = then.data {
                        unsafe { analyze_scope(&mut *inner, res, stack, false); }
                    }
                    if let Some(ref mut el) = else_ {
                        if !matches!(el.data, AstStmtData::Compound(_)) {
                            let body = std::mem::replace(&mut el.data, AstStmtData::Compound(vec![]));
                            el.data = AstStmtData::Compound(vec![AstStmt { kind: el.kind, line: el.line, col: el.col, data: body }]);
                        }
                        if let AstStmtData::Compound(ref mut inner) = el.data {
                            unsafe { analyze_scope(&mut *inner, res, stack, false); }
                        }
                    }
                    stmts[i].data = AstStmtData::If { cond, then, else_ };
                }
                i += 1;
                continue;
            }

            // ── Handle while, for, for-in: recurse into body (loop scope) ──
            let is_loop = matches!(stmts[i].kind, AstStmtKind::While | AstStmtKind::For | AstStmtKind::ForIn);
            if is_loop {
                let loop_taken = std::mem::replace(&mut stmts[i].data, AstStmtData::Expr(AstExpr { kind: AstExprKind::Int, expr_type: None, line: 0, col: 0, data: AstExprData::Int(0) }));
                match loop_taken {
                    AstStmtData::While { cond, mut body } => {
                        recurse_loop_body(&mut body, res, stack);
                        stmts[i].data = AstStmtData::While { cond, body };
                    }
                    AstStmtData::For { mut init, cond, mut incr, mut body } => {
                        // Register an object declared in the for-init (e.g. `for (Mini *o = [[Mini alloc] init]; ...)`)
                        // in the enclosing scope so it's released at scope end. Its declaration
                        // is hoisted out of the for header by codegen.
                        if let Some(ref init_stmt) = init {
                            if let AstStmtData::Decl(d) = &init_stmt.data {
                                if let AstDeclData::Variable { init: Some(ref init_val), ref var_type, .. } = d.data {
                                    if ownership_for_expr(init_val) == Ownership::Retained {
                                        if let Some(ref name) = d.name {
                                            if var_type.as_ref().map_or(false, |t| is_object_type(t)) {
                                                stack[my_idx].vars.push(name.clone());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        recurse_loop_body(&mut body, res, stack);
                        stmts[i].data = AstStmtData::For { init, cond, incr, body };
                    }
                    AstStmtData::ForIn { mut var, mut collection, mut body } => {
                        recurse_loop_body(&mut body, res, stack);
                        stmts[i].data = AstStmtData::ForIn { var, collection, body };
                    }
                    _ => {}
                }
                i += 1;
                continue;
            }

            // ── Handle @try/@catch/@finally ──
            if stmts[i].kind == AstStmtKind::Try {
                let try_taken = std::mem::replace(&mut stmts[i].data, AstStmtData::Expr(AstExpr { kind: AstExprKind::Int, expr_type: None, line: 0, col: 0, data: AstExprData::Int(0) }));
                if let AstStmtData::Try { mut try_block, mut catches, mut finally_block } = try_taken {
                    if let AstStmtData::Compound(ref mut inner) = try_block.data { unsafe { analyze_scope(&mut *inner, res, stack, false); } }
                    for c in catches.iter_mut() {
                        if let AstStmtData::Catch { ref mut body, .. } = c.data {
                            if let AstStmtData::Compound(ref mut inner) = body.data { unsafe { analyze_scope(&mut *inner, res, stack, false); } }
                        }
                    }
                    if let Some(ref mut fb) = finally_block {
                        if let AstStmtData::Compound(ref mut inner) = fb.data { unsafe { analyze_scope(&mut *inner, res, stack, false); } }
                    }
                    stmts[i].data = AstStmtData::Try { try_block, catches, finally_block };
                }
                i += 1;
                continue;
            }

            // ── Handle manual `[var release]` / `nupa_release(var)`: forget var everywhere ──
            if let AstStmtData::Expr(e) = &stmts[i].data {
                match &e.data {
                    AstExprData::MsgSend { receiver, selector, args, .. } if selector == "release" && args.is_empty() => {
                        if let AstExprData::VarRef { name, .. } = &receiver.data {
                            drop_var(stack, name);
                        }
                    }
                    AstExprData::FuncCall { name, args, .. } if name == "nupa_release" && args.len() == 1 => {
                        if let AstExprData::VarRef { name, .. } = &args[0].data {
                            drop_var(stack, name);
                        }
                    }
                    _ => {}
                }
            }

            i += 1;
        }

        // ── End of scope: release this scope's own remaining vars ──
        let own: Vec<String> = stack[my_idx].vars.clone();
        if !own.is_empty() {
            let insert_pos = if is_return_stmt(stmts.last().unwrap()) { stmts.len() - 1 } else { stmts.len() };
            for name in &own {
                stmts.insert(insert_pos, make_release_stmt(&var_ref_expr(name)));
            }
        }
        stack.pop();
    }

    fn clear_all(stack: &mut Vec<Scope>) {
        for s in stack.iter_mut() { s.vars.clear(); }
    }

    fn drop_var(stack: &mut Vec<Scope>, name: &str) {
        for s in stack.iter_mut() {
            s.vars.retain(|v| v != name);
        }
    }

    fn recurse_scope(inner: &mut Vec<AstStmt>, res: &mut ArcResult, stack: &mut Vec<Scope>) {
        unsafe { analyze_scope(inner, res, stack, false); }
    }

    fn recurse_loop_body(body: &mut AstStmt, res: &mut ArcResult, stack: &mut Vec<Scope>) {
        // Wrap non-compound bodies so releases are inserted in a scope.
        let inner: *mut Vec<AstStmt> = match body.data {
            AstStmtData::Compound(ref mut inner) => inner as *mut Vec<AstStmt>,
            _ => {
                let wrapped = std::mem::replace(&mut body.data, AstStmtData::Compound(vec![]));
                if let AstStmtData::Compound(ref mut v) = body.data {
                    v.push(AstStmt { kind: body.kind, line: body.line, col: body.col, data: wrapped });
                }
                match body.data {
                    AstStmtData::Compound(ref mut inner) => inner as *mut Vec<AstStmt>,
                    _ => std::ptr::null_mut(),
                }
            }
        };
        if !inner.is_null() {
            unsafe { analyze_scope(&mut *inner, res, stack, true); }
        }
    }

    let mut stack: Vec<Scope> = Vec::new();
    analyze_scope(stmts, &mut res, &mut stack, false);
    res
}

// ─── global analysis (placeholder) ─────────────────────────────────────────

pub fn arc_global_analyze(_cfg: &Cfg, _res: &mut ArcResult, _method_name: &str) {}

// ─── loop analysis (placeholder) ───────────────────────────────────────────

pub fn arc_analyze_loops(_cfg: &Cfg, _res: &mut ArcResult, _method_name: &str) {}

// ─── retain/release insertion (no-op now, analysis inserts directly) ───────

pub fn arc_insert_actions(_body: &mut AstStmt, _res: &ArcResult) {}

// ─── redundant pair optimization ───────────────────────────────────────────

pub fn arc_optimize_pairs(body: &mut AstStmt) {
    let stmts = match &mut body.data {
        AstStmtData::Compound(ref mut s) => s,
        _ => return,
    };
    let mut remove: Vec<usize> = Vec::new();
    for i in 0..stmts.len().saturating_sub(1) {
        if remove.contains(&i) { continue; }
        if let Some(j) = is_retain_release_pair(&stmts[i], &stmts[i + 1]) {
            if j { remove.push(i); remove.push(i + 1); }
        }
    }
    if remove.is_empty() { return; }
    let mut write = 0;
    for i in 0..stmts.len() {
        if !remove.contains(&i) {
            stmts[write] = stmts[i].clone();
            write += 1;
        }
    }
    stmts.truncate(write);
}

fn is_retain_release_pair(s1: &AstStmt, s2: &AstStmt) -> Option<bool> {
    let e1 = match &s1.data { AstStmtData::Expr(e) => e, _ => return None };
    let e2 = match &s2.data { AstStmtData::Expr(e) => e, _ => return None };
    let (name1, args1) = match &e1.data { AstExprData::FuncCall { name, args, .. } => (name, args), _ => return None };
    let (name2, args2) = match &e2.data { AstExprData::FuncCall { name, args, .. } => (name, args), _ => return None };
    if args1.len() != 1 || args2.len() != 1 { return None; }
    let t1 = match &args1[0].data { AstExprData::VarRef { name, .. } => name, _ => return None };
    let t2 = match &args2[0].data { AstExprData::VarRef { name, .. } => name, _ => return None };
    if t1 != t2 { return None; }
    let is_retain = |n: &str| n == "nupa_retain";
    let is_release = |n: &str| n == "nupa_release";
    if (is_retain(name1) && is_release(name2)) || (is_release(name1) && is_retain(name2)) {
        Some(true)
    } else {
        None
    }
}