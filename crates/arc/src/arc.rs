use nupa_ast::*;
use nupa_cfg::*;
use nupa_ownership::*;

#[derive(Debug, Clone, Copy)]
pub enum ArcActionKind {
    Retain, Release, Autorelease,
}

#[derive(Debug, Clone)]
pub struct ArcAction {
    pub kind: ArcActionKind,
    pub target: Option<Box<AstExpr>>,
    pub insert_after: Option<Box<AstStmt>>,
    pub insert_at_end: bool,
}

#[derive(Debug, Clone)]
pub struct ArcResult {
    pub actions: Vec<ArcAction>,
}

impl ArcResult {
    pub fn new() -> Self { ArcResult { actions: Vec::new() } }
}

fn add_action(res: &mut ArcResult, kind: ArcActionKind, target: Option<Box<AstExpr>>, after: Option<Box<AstStmt>>) {
    res.actions.push(ArcAction { kind, target, insert_after: after, insert_at_end: false });
}

// ─── lifetime tracking ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct VarLifetime {
    var_expr: AstExpr,
    ownership: Ownership,
    last_use_idx: usize,
    is_param: bool,
    is_self: bool,
    released: bool,
}

// ─── expression scanning ───────────────────────────────────────────────────

fn collect_used_exprs<'a>(e: &'a AstExpr, out: &mut Vec<&'a AstExpr>) {
    out.push(e);
    match &e.data {
        AstExprData::VarRef { .. } | AstExprData::IvarRef { .. } | AstExprData::PropRef { .. } => {}
        AstExprData::MsgSend { receiver, args, .. } => {
            collect_used_exprs(receiver, out);
            for a in args { collect_used_exprs(a, out); }
        }
        AstExprData::FuncCall { args, .. } => {
            for a in args { collect_used_exprs(a, out); }
        }
        AstExprData::Unary { operand, .. } => collect_used_exprs(operand, out),
        AstExprData::Binary { left, right, .. } => {
            collect_used_exprs(left, out);
            collect_used_exprs(right, out);
        }
        AstExprData::Assign { target, value } => {
            collect_used_exprs(target, out);
            collect_used_exprs(value, out);
        }
        AstExprData::Cast { expr, .. } => collect_used_exprs(expr, out),
        AstExprData::Comma(exprs) => { for e in exprs { collect_used_exprs(e, out); } }
        AstExprData::Ternary { cond, then, else_ } => {
            collect_used_exprs(cond, out);
            collect_used_exprs(then, out);
            collect_used_exprs(else_, out);
        }
        AstExprData::Subscript { object, key } => {
            collect_used_exprs(object, out);
            collect_used_exprs(key, out);
        }
        _ => {}
    }
}

fn scan_stmt_exprs<'a>(s: &'a AstStmt, out: &mut Vec<&'a AstExpr>) {
    match &s.data {
        AstStmtData::Expr(e) => collect_used_exprs(e, out),
        AstStmtData::Return(Some(e)) => collect_used_exprs(e, out),
        AstStmtData::Decl(d) => {
            if let AstDeclData::Variable { init: Some(ref i), .. } = d.data {
                collect_used_exprs(i, out);
            }
        }
        AstStmtData::Compound(stmts) => {
            for st in stmts { scan_stmt_exprs(st, out); }
        }
        AstStmtData::If { cond, then, else_ } => {
            collect_used_exprs(cond, out);
            scan_stmt_exprs(then, out);
            if let Some(el) = else_ { scan_stmt_exprs(el, out); }
        }
        AstStmtData::While { cond, body } => {
            collect_used_exprs(cond, out);
            scan_stmt_exprs(body, out);
        }
        AstStmtData::For { init, cond, incr, body } => {
            if let Some(i) = init { scan_stmt_exprs(i, out); }
            if let Some(c) = cond { collect_used_exprs(c, out); }
            if let Some(i) = incr { collect_used_exprs(i, out); }
            scan_stmt_exprs(body, out);
        }
        AstStmtData::ForIn { var, collection, body } => {
            collect_used_exprs(var, out);
            collect_used_exprs(collection, out);
            scan_stmt_exprs(body, out);
        }
        _ => {}
    }
}

fn is_scalar(e: &AstExpr) -> bool {
    matches!(e.kind, AstExprKind::Int | AstExprKind::Float | AstExprKind::Bool | AstExprKind::Nil | AstExprKind::Null | AstExprKind::Sizeof)
}

fn is_var_ref(e: &AstExpr) -> bool {
    matches!(e.kind, AstExprKind::VarRef)
}

// ─── local analysis ────────────────────────────────────────────────────────

pub fn arc_local_analyze(body: &AstStmt, _cfg: &Cfg, method_name: &str) -> ArcResult {
    let mut res = ArcResult::new();
    let return_ownership = ownership_for_method(method_name);

    let stmts = match &body.data {
        AstStmtData::Compound(stmts) => stmts,
        _ => return res,
    };

    // Lifetime tracking for retained variables
    let mut lifetimes: Vec<VarLifetime> = Vec::new();

    // First pass: find var decls initialized with retained values
    for (i, s) in stmts.iter().enumerate() {
        if let AstStmtData::Decl(d) = &s.data {
            if let AstDeclData::Variable { init: Some(ref init_val), .. } = d.data {
                let ow = ownership_for_expr(init_val);
                if ow == Ownership::Retained {
                    let var_ref = AstExpr {
                        kind: AstExprKind::VarRef, expr_type: None, line: 0, col: 0,
                        data: AstExprData::VarRef { sym: None, name: d.name.clone().unwrap_or_default() },
                    };
                    lifetimes.push(VarLifetime {
                        var_expr: var_ref, ownership: ow,
                        last_use_idx: i, is_param: false, is_self: false, released: false,
                    });
                }
            }
        }
    }

    // Second pass: find retained expressions that need release
    for (i, s) in stmts.iter().enumerate() {
        let mut exprs: Vec<&AstExpr> = Vec::new();
        scan_stmt_exprs(s, &mut exprs);

        for e in &exprs {
            if is_scalar(e) { continue; }
            let ow = ownership_for_expr(e);
            if ow != Ownership::Retained { continue; }

            // Check if this expression is assigned to a variable
            let mut is_stored = false;
            if let AstStmtData::Expr(ex) = &s.data {
                if let AstExprData::Assign { target, value } = &ex.data {
                    if let AstExprData::VarRef { name: vname, .. } = &value.data {
                        if let AstExprData::VarRef { name: tname, .. } = &target.data {
                            // Check if the target is one of our tracked variables
                            for lt in &mut lifetimes {
                                if let AstExprData::VarRef { name: ltname, .. } = &lt.var_expr.data {
                                    if ltname == tname {
                                        is_stored = true;
                                        lt.last_use_idx = i;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if is_stored { continue; }

            // Check if this is a return expression
            if let AstStmtData::Return(Some(_)) = &s.data {
                if return_ownership == Ownership::Retained { continue; }
            }

            // Not stored — needs release
            add_action(&mut res, ArcActionKind::Release, Some(Box::new((*e).clone())), Some(Box::new(s.clone())));
        }

        // Update last use for tracked variables
        for e in &exprs {
            if let AstExprData::VarRef { name: ref ename, .. } = &e.data {
                for lt in &mut lifetimes {
                    if let AstExprData::VarRef { name: ref ltname, .. } = &lt.var_expr.data {
                        if ltname == ename { lt.last_use_idx = i; }
                    }
                }
            }
        }
    }

    // Third pass: release tracked variables after their last use
    for lt in &lifetimes {
        if lt.released || lt.is_self || lt.is_param { continue; }
        if lt.ownership != Ownership::Retained { continue; }
        if lt.last_use_idx < stmts.len() {
            if let AstStmtData::Return(_) = &stmts[lt.last_use_idx].data { continue; }
            add_action(&mut res, ArcActionKind::Release, Some(Box::new(lt.var_expr.clone())), Some(Box::new(stmts[lt.last_use_idx].clone())));
        }
    }

    res
}

// ─── global analysis (placeholder, matches C version) ──────────────────────

pub fn arc_global_analyze(_cfg: &Cfg, _res: &mut ArcResult, _method_name: &str) {}

// ─── loop analysis (placeholder, matches C version) ────────────────────────

pub fn arc_analyze_loops(_cfg: &Cfg, _res: &mut ArcResult, _method_name: &str) {}

// ─── retain/release insertion ──────────────────────────────────────────────

fn make_nupa_call(kind: ArcActionKind, target: &AstExpr) -> AstExpr {
    let name = match kind {
        ArcActionKind::Retain => "nupa_retain",
        ArcActionKind::Release => "nupa_release",
        ArcActionKind::Autorelease => "nupa_autorelease",
    };
    AstExpr {
        kind: AstExprKind::FuncCall, expr_type: None, line: 0, col: 0,
        data: AstExprData::FuncCall {
            func: None, name: name.to_string(), callee: None, args: vec![target.clone()],
        },
    }
}

pub fn arc_insert_actions(body: &mut AstStmt, res: &ArcResult) {
    let stmts = match &mut body.data {
        AstStmtData::Compound(ref mut s) => s,
        _ => return,
    };

    // Insert from last to first to avoid index invalidation
    for action in res.actions.iter().rev() {
        if let Some(ref after) = action.insert_after {
            let pos = stmts.iter().position(|s| {
                std::ptr::eq(s as *const AstStmt, after.as_ref() as *const AstStmt)
            });
            if let Some(p) = pos {
                if let Some(ref target) = action.target {
                    let call = make_nupa_call(action.kind, target);
                    let new_stmt = AstStmt {
                        kind: AstStmtKind::Expr, line: 0, col: 0,
                        data: AstStmtData::Expr(call),
                    };
                    stmts.insert(p + 1, new_stmt);
                }
            }
        }
    }

    // Handle insert_at_end actions
    for action in &res.actions {
        if action.insert_at_end {
            if let Some(ref target) = action.target {
                let call = make_nupa_call(action.kind, target);
                let new_stmt = AstStmt {
                    kind: AstStmtKind::Expr, line: 0, col: 0,
                    data: AstStmtData::Expr(call),
                };
                stmts.push(new_stmt);
            }
        }
    }
}

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
    // Check same target (same name suggests same variable)
    let t1 = match &args1[0].data { AstExprData::VarRef { name, .. } => name, _ => return None };
    let t2 = match &args2[0].data { AstExprData::VarRef { name, .. } => name, _ => return None };
    if t1 != t2 { return None; }
    // Check retain/release or release/retain pair
    let is_retain = |n: &str| n == "nupa_retain";
    let is_release = |n: &str| n == "nupa_release";
    if (is_retain(name1) && is_release(name2)) || (is_release(name1) && is_retain(name2)) {
        Some(true)
    } else {
        None
    }
}