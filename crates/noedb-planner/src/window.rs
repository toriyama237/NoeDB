//! Window function planning helpers (Phase 5 Weeks 37–38).

use noedb_ast::{ColumnRef, Expr, Ident, SelectItem, WindowFunc};

use crate::logical::{LogicalPlan, WindowCompute};

/// Split `SELECT` items into window ops and rewritten projection list.
#[must_use]
pub fn split_window_items(items: &[SelectItem]) -> (Vec<WindowCompute>, Vec<SelectItem>) {
    let mut windows = Vec::new();
    let mut projected = Vec::new();
    for item in items {
        if let Expr::Function {
            name,
            args,
            over: Some(spec),
            span,
        } = &item.expr
        {
            let func_and_arg = if args.is_empty() {
                WindowFunc::parse_name(&name.value).map(|f| (f, None))
            } else if args.len() == 1 {
                WindowFunc::parse_agg_name(&name.value).map(|f| (f, Some(args[0].clone())))
            } else {
                None
            };
            let Some((func, arg)) = func_and_arg else {
                projected.push(item.clone());
                continue;
            };
            let output_name = item
                .alias
                .as_ref()
                .map_or_else(|| name.value.to_ascii_lowercase(), |a| a.value.clone());
            windows.push(WindowCompute {
                func,
                arg,
                spec: spec.clone(),
                output_name: output_name.clone(),
            });
            projected.push(SelectItem {
                expr: Expr::Column(ColumnRef::Named {
                    table: None,
                    column: Ident::new(output_name, *span),
                }),
                alias: item.alias.clone(),
            });
            continue;
        }
        projected.push(item.clone());
    }
    (windows, projected)
}

/// Insert [`LogicalPlan::Window`] when needed.
#[must_use]
pub fn wrap_window(plan: LogicalPlan, items: &[SelectItem]) -> (LogicalPlan, Vec<SelectItem>) {
    let (windows, projected) = split_window_items(items);
    if windows.is_empty() {
        return (plan, projected);
    }
    let plan = LogicalPlan::Window {
        input: Box::new(plan),
        windows,
    };
    (plan, projected)
}
