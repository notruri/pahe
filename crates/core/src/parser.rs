use swc_ecma_ast::*;
use swc_ecma_visit::{Visit, VisitWith};

#[derive(Debug)]
pub struct PackedCall {
    pub payload: String,
    pub radix: usize,
    pub count: usize,
    pub symbols: Option<Vec<String>>,
}

pub struct Finder {
    results: Vec<PackedCall>,
}

impl Finder {
    fn parse_call(&mut self, call: &CallExpr) {
        if call.args.len() == 6 {
            if let (Some(payload_arg), Some(radix_arg), Some(count_arg), Some(sym_arg)) = (
                call.args.get(0),
                call.args.get(1),
                call.args.get(2),
                call.args.get(3),
            ) {
                if let (
                    Expr::Lit(Lit::Str(payload_str)),
                    Expr::Lit(Lit::Num(radix_num)),
                    Expr::Lit(Lit::Num(count_num)),
                    Expr::Call(sym_call),
                ) = (
                    &*payload_arg.expr,
                    &*radix_arg.expr,
                    &*count_arg.expr,
                    &*sym_arg.expr,
                ) {
                    let payload = payload_str.value.to_string_lossy().to_string();
                    let radix = radix_num.value;
                    let count = count_num.value;
                    let symbols = self.parse_symbols(sym_call);

                    self.results.push(PackedCall {
                        payload,
                        radix: radix as usize,
                        count: count as usize,
                        symbols,
                    });
                }
            }
        }
    }

    fn parse_symbols(&self, call: &CallExpr) -> Option<Vec<String>> {
        if let Callee::Expr(sym_call) = &call.callee {
            if let Expr::Member(sym_mem) = &**sym_call {
                if let Expr::Lit(Lit::Str(sym_str)) = &*sym_mem.obj {
                    return Some(
                        sym_str
                            .value
                            .to_string_lossy()
                            .split('|')
                            .filter_map(|s| (!s.trim().is_empty()).then_some(s.to_string()))
                            .collect(),
                    );
                }
            }
        }
        None
    }
}

impl Visit for Finder {
    fn visit_call_expr(&mut self, n: &CallExpr) {
        // Step 1: match eval(...)
        if let Callee::Expr(callee_expr) = &n.callee {
            if let Expr::Ident(ident) = &**callee_expr {
                if ident.sym == *"eval" {
                    // Step 2: check first argument expr
                    if let Some(first_arg) = n.args.get(0) {
                        if let Expr::Call(call) = &*first_arg.expr {
                            // Step 3: parse call expression
                            self.parse_call(call);
                        }
                    }
                }
            }
        }

        // continue walking
        n.visit_children_with(self);
    }
}
