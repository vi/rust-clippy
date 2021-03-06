use crate::rustc::lint::{LateContext, LateLintPass, LintArray, LintPass};
use crate::rustc::{declare_tool_lint, lint_array};
use if_chain::if_chain;
use crate::rustc::hir;
use crate::rustc::hir::intravisit::{walk_expr, NestedVisitorMap, Visitor};
use crate::syntax::ast;
use crate::utils::{get_trait_def_id, span_lint};

/// **What it does:** Lints for suspicious operations in impls of arithmetic operators, e.g.
/// subtracting elements in an Add impl.
///
/// **Why this is bad?** This is probably a typo or copy-and-paste error and not intended.
///
/// **Known problems:** None.
///
/// **Example:**
/// ```rust
/// impl Add for Foo {
///     type Output = Foo;
///
///     fn add(self, other: Foo) -> Foo {
///         Foo(self.0 - other.0)
///     }
/// }
/// ```
declare_clippy_lint! {
    pub SUSPICIOUS_ARITHMETIC_IMPL,
    correctness,
    "suspicious use of operators in impl of arithmetic trait"
}

/// **What it does:** Lints for suspicious operations in impls of OpAssign, e.g.
/// subtracting elements in an AddAssign impl.
///
/// **Why this is bad?** This is probably a typo or copy-and-paste error and not intended.
///
/// **Known problems:** None.
///
/// **Example:**
/// ```rust
/// impl AddAssign for Foo {
///     fn add_assign(&mut self, other: Foo) {
///         *self = *self - other;
///     }
/// }
/// ```
declare_clippy_lint! {
    pub SUSPICIOUS_OP_ASSIGN_IMPL,
    correctness,
    "suspicious use of operators in impl of OpAssign trait"
}

#[derive(Copy, Clone)]
pub struct SuspiciousImpl;

impl LintPass for SuspiciousImpl {
    fn get_lints(&self) -> LintArray {
        lint_array![SUSPICIOUS_ARITHMETIC_IMPL, SUSPICIOUS_OP_ASSIGN_IMPL]
    }
}

impl<'a, 'tcx> LateLintPass<'a, 'tcx> for SuspiciousImpl {
    fn check_expr(&mut self, cx: &LateContext<'a, 'tcx>, expr: &'tcx hir::Expr) {
        if let hir::ExprKind::Binary(binop, _, _) = expr.node {
            match binop.node {
                | hir::BinOpKind::Eq
                | hir::BinOpKind::Lt
                | hir::BinOpKind::Le
                | hir::BinOpKind::Ne
                | hir::BinOpKind::Ge
                | hir::BinOpKind::Gt
                => return,
                _ => {},
            }
            // Check if the binary expression is part of another bi/unary expression
            // as a child node
            let mut parent_expr = cx.tcx.hir.get_parent_node(expr.id);
            while parent_expr != ast::CRATE_NODE_ID {
                if let hir::Node::Expr(e) = cx.tcx.hir.get(parent_expr) {
                    match e.node {
                        hir::ExprKind::Binary(..)
                        | hir::ExprKind::Unary(hir::UnOp::UnNot, _)
                        | hir::ExprKind::Unary(hir::UnOp::UnNeg, _) => return,
                        _ => {},
                    }
                }
                parent_expr = cx.tcx.hir.get_parent_node(parent_expr);
            }
            // as a parent node
            let mut visitor = BinaryExprVisitor {
                in_binary_expr: false,
            };
            walk_expr(&mut visitor, expr);

            if visitor.in_binary_expr {
                return;
            }

            if let Some(impl_trait) = check_binop(
                cx,
                expr,
                binop.node,
                &["Add", "Sub", "Mul", "Div"],
                &[
                    hir::BinOpKind::Add,
                    hir::BinOpKind::Sub,
                    hir::BinOpKind::Mul,
                    hir::BinOpKind::Div,
                ],
            ) {
                span_lint(
                    cx,
                    SUSPICIOUS_ARITHMETIC_IMPL,
                    binop.span,
                    &format!(
                        r#"Suspicious use of binary operator in `{}` impl"#,
                        impl_trait
                    ),
                );
            }

            if let Some(impl_trait) = check_binop(
                cx,
                expr,
                binop.node,
                &[
                    "AddAssign",
                    "SubAssign",
                    "MulAssign",
                    "DivAssign",
                    "BitAndAssign",
                    "BitOrAssign",
                    "BitXorAssign",
                    "RemAssign",
                    "ShlAssign",
                    "ShrAssign",
                ],
                &[
                    hir::BinOpKind::Add,
                    hir::BinOpKind::Sub,
                    hir::BinOpKind::Mul,
                    hir::BinOpKind::Div,
                    hir::BinOpKind::BitAnd,
                    hir::BinOpKind::BitOr,
                    hir::BinOpKind::BitXor,
                    hir::BinOpKind::Rem,
                    hir::BinOpKind::Shl,
                    hir::BinOpKind::Shr,
                ],
            ) {
                span_lint(
                    cx,
                    SUSPICIOUS_OP_ASSIGN_IMPL,
                    binop.span,
                    &format!(
                        r#"Suspicious use of binary operator in `{}` impl"#,
                        impl_trait
                    ),
                );
            }
        }
    }
}

fn check_binop<'a>(
    cx: &LateContext<'_, '_>,
    expr: &hir::Expr,
    binop: hir::BinOpKind,
    traits: &[&'a str],
    expected_ops: &[hir::BinOpKind],
) -> Option<&'a str> {
    let mut trait_ids = vec![];
    let [krate, module] = crate::utils::paths::OPS_MODULE;

    for t in traits {
        let path = [krate, module, t];
        if let Some(trait_id) = get_trait_def_id(cx, &path) {
            trait_ids.push(trait_id);
        } else {
            return None;
        }
    }

    // Get the actually implemented trait
    let parent_fn = cx.tcx.hir.get_parent(expr.id);
    let parent_impl = cx.tcx.hir.get_parent(parent_fn);

    if_chain! {
        if parent_impl != ast::CRATE_NODE_ID;
        if let hir::Node::Item(item) = cx.tcx.hir.get(parent_impl);
        if let hir::ItemKind::Impl(_, _, _, _, Some(ref trait_ref), _, _) = item.node;
        if let Some(idx) = trait_ids.iter().position(|&tid| tid == trait_ref.path.def.def_id());
        if binop != expected_ops[idx];
        then{
            return Some(traits[idx])
        }
    }

    None
}

struct BinaryExprVisitor {
    in_binary_expr: bool,
}

impl<'a, 'tcx: 'a> Visitor<'tcx> for BinaryExprVisitor {
    fn visit_expr(&mut self, expr: &'tcx hir::Expr) {
        match expr.node {
            hir::ExprKind::Binary(..)
            | hir::ExprKind::Unary(hir::UnOp::UnNot, _)
            | hir::ExprKind::Unary(hir::UnOp::UnNeg, _) => {
                self.in_binary_expr = true
            },
            _ => {},
        }

        walk_expr(self, expr);
    }
    fn nested_visit_map<'this>(&'this mut self) -> NestedVisitorMap<'this, 'tcx> {
        NestedVisitorMap::None
    }
}
