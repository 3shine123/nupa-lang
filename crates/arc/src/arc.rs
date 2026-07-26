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

// Insert releases before a statement at index `pos`, skipping any variable
// that is being returned/thrown. Returns the number of releases inserted.
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
        let var_ref = AstExpr {
            kind: AstExprKind::VarRef, expr_type: None, line: 0, col: 0,
            data: AstExprData::VarRef { sym: None, name: name.clone() },
        };
        stmts.insert(pos, make_release_stmt(&var_ref));
        vars.remove(i);
        count += 1;
    }
    count
}

// Insert releases at the end of the statement list
fn insert_releases_at_end(stmts: &mut Vec<AstStmt>, vars: &[String]) {
    for name in vars {
        let var_ref = AstExpr {
            kind: AstExprKind::VarRef, expr_type: None, line: 0, col: 0,
            data: AstExprData::VarRef { sym: None, name: name.clone() },
        };
        stmts.push(make_release_stmt(&var_ref));
    }
}

// ─── local analysis (directly inserts releases into AST) ───────────────────

pub fn arc_local_analyze(body: &mut AstStmt, _cfg: &Cfg, method_name: &str) -> ArcResult {
    let return_ownership = ownership_for_method(method_name);
    let mut res = ArcResult::new();

    let stmts = match body.data {
        AstStmtData::Compound(ref mut stmts) => stmts,
        _ => return res,
    };

    // Recursively analyze scope, inserting releases directly.
    // `parent_vars` are variables from enclosing scopes that also need release.
    fn analyze_scope(stmts: &mut Vec<AstStmt>, return_ownership: Ownership, res: &mut ArcResult, parent_vars: &[String]) {
        let mut release_vars: Vec<String> = parent_vars.to_vec();

        // Helper: get all active variables (parent + current)
        let all_vars = || -> &Vec<String> { &release_vars };

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
                    unsafe { analyze_scope(&mut *inner, return_ownership, res, &release_vars); }
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
                                release_vars.push(name.clone());
                            }
                        }
                    }
                }
                i += 1;
                continue;
            }

            // ─── Handle return: insert releases before it ──
            let is_return = matches!(stmts[i].data, AstStmtData::Return(_));
            if is_return {
                let ret_expr_clone = match &stmts[i].data { AstStmtData::Return(e) => e.clone(), _ => None };
                let n = insert_releases_before(stmts, i, &mut release_vars, ret_expr_clone.as_ref().map(|e| e.as_ref()));
                i += 1 + n;
                continue;
            }

            // ── Handle throw: insert releases before it (exempt thrown variable) ──
            if matches!(stmts[i].data, AstStmtData::Throw(_)) {
                let throw_expr = match &stmts[i].data { AstStmtData::Throw(e) => e.clone(), _ => None };
                let n = insert_releases_before(stmts, i, &mut release_vars, throw_expr.as_ref().map(|e| e.as_ref()));
                i += 1 + n;
                continue;
            }

            // ── Handle break/continue: insert releases before it ──
            if matches!(stmts[i].kind, AstStmtKind::Break | AstStmtKind::Continue) {
                let n = insert_releases_before(stmts, i, &mut release_vars, None);
                i += 1 + n;
                continue;
            }

            // ── Handle if/else: recurse into branches ──
            if stmts[i].kind == AstStmtKind::If {
                let if_taken = std::mem::replace(&mut stmts[i].data, AstStmtData::Expr(AstExpr { kind: AstExprKind::Int, expr_type: None, line: 0, col: 0, data: AstExprData::Int(0) }));
                if let AstStmtData::If { cond, mut then, mut else_ } = if_taken {
                    // Ensure branches are Compound (wrap single statements)
                    if !matches!(then.data, AstStmtData::Compound(_)) {
                        let body = std::mem::replace(&mut then.data, AstStmtData::Compound(vec![]));
                        then.data = AstStmtData::Compound(vec![AstStmt { kind: then.kind, line: then.line, col: then.col, data: body }]);
                    }
                    if let AstStmtData::Compound(ref mut inner) = then.data {
                        unsafe { analyze_scope(&mut *inner, return_ownership, res, &release_vars); }
                    }
                    if let Some(ref mut el) = else_ {
                        if !matches!(el.data, AstStmtData::Compound(_)) {
                            let body = std::mem::replace(&mut el.data, AstStmtData::Compound(vec![]));
                            el.data = AstStmtData::Compound(vec![AstStmt { kind: el.kind, line: el.line, col: el.col, data: body }]);
                        }
                        if let AstStmtData::Compound(ref mut inner) = el.data {
                            unsafe { analyze_scope(&mut *inner, return_ownership, res, &release_vars); }
                        }
                    }
                    stmts[i].data = AstStmtData::If { cond, then, else_ };
                }
                i += 1;
                continue;
            }

            // ── Handle while, for, for-in: recurse into body ──
            let is_loop = matches!(stmts[i].kind, AstStmtKind::While | AstStmtKind::For | AstStmtKind::ForIn);
            if is_loop {
                let loop_taken = std::mem::replace(&mut stmts[i].data, AstStmtData::Expr(AstExpr { kind: AstExprKind::Int, expr_type: None, line: 0, col: 0, data: AstExprData::Int(0) }));
                match loop_taken {
                    AstStmtData::While { cond, mut body } => {
                        if let AstStmtData::Compound(ref mut inner) = body.data { unsafe { analyze_scope(&mut *inner, return_ownership, res, &release_vars); } }
                        stmts[i].data = AstStmtData::While { cond, body };
                    }
                    AstStmtData::For { mut init, cond, mut incr, mut body } => {
                        if let AstStmtData::Compound(ref mut inner) = body.data { unsafe { analyze_scope(&mut *inner, return_ownership, res, &release_vars); } }
                        stmts[i].data = AstStmtData::For { init, cond, incr, body };
                    }
                    AstStmtData::ForIn { mut var, mut collection, mut body } => {
                        if let AstStmtData::Compound(ref mut inner) = body.data { unsafe { analyze_scope(&mut *inner, return_ownership, res, &release_vars); } }
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
                    if let AstStmtData::Compound(ref mut inner) = try_block.data { unsafe { analyze_scope(&mut *inner, return_ownership, res, &release_vars); } }
                    for c in catches.iter_mut() {
                        if let AstStmtData::Catch { ref mut body, .. } = c.data {
                            if let AstStmtData::Compound(ref mut inner) = body.data { unsafe { analyze_scope(&mut *inner, return_ownership, res, &release_vars); } }
                        }
                    }
                    if let Some(ref mut fb) = finally_block {
                        if let AstStmtData::Compound(ref mut inner) = fb.data { unsafe { analyze_scope(&mut *inner, return_ownership, res, &release_vars); } }
                    }
                    stmts[i].data = AstStmtData::Try { try_block, catches, finally_block };
                }
                i += 1;
                continue;
            }

            // ── Handle manual `[var release]` or `nupa_release(var)`: remove var from tracking ──
            if let AstStmtData::Expr(e) = &stmts[i].data {
                match &e.data {
                    AstExprData::MsgSend { receiver, selector, args, .. } if selector == "release" && args.is_empty() => {
                        if let AstExprData::VarRef { name, .. } = &receiver.data {
                            release_vars.retain(|v| *v != *name);
                        }
                    }
                    AstExprData::FuncCall { name, args, .. } if name == "nupa_release" && args.len() == 1 => {
                        if let AstExprData::VarRef { name, .. } = &args[0].data {
                            release_vars.retain(|v| *v != *name);
                        }
                    }
                    _ => {}
                }
            }

            i += 1;
        }

        // Insert releases at end of scope for remaining variables (before trailing Return)
        if !release_vars.is_empty() && !stmts.is_empty() {
            let insert_pos = if stmts.len() >= 2 {
                let last = &stmts[stmts.len() - 1];
                if is_return_stmt(last) {
                    // Don't release variables that are being returned
                    if let AstStmtData::Return(Some(ref e)) = last.data {
                        if let Some(rv) = extract_returned_var(e) {
                            release_vars.retain(|v| v != rv);
                        }
                    }
                    stmts.len() - 1
                } else {
                    stmts.len()
                }
            } else {
                stmts.len()
            };
            for name in &release_vars {
                let var_ref = AstExpr {
                    kind: AstExprKind::VarRef, expr_type: None, line: 0, col: 0,
                    data: AstExprData::VarRef { sym: None, name: name.clone() },
                };
                stmts.insert(insert_pos, make_release_stmt(&var_ref));
            }
        }
    }

    analyze_scope(stmts, return_ownership, &mut res, &[]);
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