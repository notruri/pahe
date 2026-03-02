use std::path::Path;

use swc_common::FileName;
use swc_common::SourceMap;
use swc_common::sync::Lrc;
use swc_ecma_ast::*;
use swc_ecma_parser::{Parser, StringInput, Syntax, lexer::Lexer};
use swc_ecma_visit::{Visit, VisitWith};

use crate::errors::*;

#[derive(Debug, Clone)]
pub struct PackedCall {
    pub payload: String,
    pub radix: usize,
    pub count: usize,
    pub symbols: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct Variable {
    pub ident: String,
    pub value: String,
}

pub struct EmbedPayloadFinder {
    results: Vec<PackedCall>,
}

pub struct VariableFinder {
    results: Vec<Variable>,
}

impl EmbedPayloadFinder {
    fn new() -> Self {
        Self {
            results: Vec::new(),
        }
    }

    fn parse_call(&mut self, call: &CallExpr) {
        if call.args.len() == 6
            && let (Some(payload_arg), Some(radix_arg), Some(count_arg), Some(sym_arg)) = (
                call.args.first(),
                call.args.get(1),
                call.args.get(2),
                call.args.get(3),
            )
            && let (
                Expr::Lit(Lit::Str(payload_str)),
                Expr::Lit(Lit::Num(radix_num)),
                Expr::Lit(Lit::Num(count_num)),
                Expr::Call(sym_call),
            ) = (
                &*payload_arg.expr,
                &*radix_arg.expr,
                &*count_arg.expr,
                &*sym_arg.expr,
            )
        {
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

    fn parse_symbols(&self, call: &CallExpr) -> Option<Vec<String>> {
        if let Callee::Expr(sym_call) = &call.callee
            && let Expr::Member(sym_mem) = &**sym_call
            && let Expr::Lit(Lit::Str(sym_str)) = &*sym_mem.obj
        {
            return Some(
                sym_str
                    .value
                    .to_string_lossy()
                    .split('|')
                    .map(|s| s.to_string())
                    .collect(),
            );
        }
        None
    }
}

impl Visit for EmbedPayloadFinder {
    fn visit_call_expr(&mut self, n: &CallExpr) {
        // Step 1: match eval(...)
        if let Callee::Expr(callee_expr) = &n.callee
            && let Expr::Ident(ident) = &**callee_expr
            && ident.sym == *"eval"
        {
            // Step 2: check first argument expr
            if let Some(first_arg) = n.args.first()
                && let Expr::Call(call) = &*first_arg.expr
            {
                // Step 3: parse call expression
                self.parse_call(call);
            }
        }

        // continue walking
        n.visit_children_with(self);
    }
}

impl Default for VariableFinder {
    fn default() -> Self {
        Self::new()
    }
}

impl VariableFinder {
    pub fn new() -> Self {
        Self {
            results: Vec::new(),
        }
    }
}

impl Visit for VariableFinder {
    // this currently only parses string declaration like
    // const variable = "value"
    fn visit_var_decl(&mut self, node: &VarDecl) {
        for decl in &node.decls {
            if let Pat::Ident(ident) = &decl.name
                && let Some(init) = &decl.init
                && let Expr::Lit(Lit::Str(lit)) = &**init
            {
                let ident = ident.sym.to_string();
                let value = lit.value.to_string_lossy().to_string();
                self.results.push(Variable { ident, value });
            }
        }

        node.visit_children_with(self);
    }
}

pub fn parse_embed_payload(payload: impl AsRef<str>) -> Result<Vec<PackedCall>> {
    let cm: Lrc<SourceMap> = Default::default();
    let fm = cm.new_source_file(
        FileName::Custom("uwu.js".into()).into(),
        payload.as_ref().to_string(),
    );
    let lexer = Lexer::new(
        Syntax::Es(Default::default()),
        Default::default(),
        StringInput::from(&*fm),
        None,
    );
    let mut parser = Parser::new_from(lexer);
    let module = parser
        .parse_module()
        .map_err(|e| ParserError::SyntaxError {
            context: "parse embed payload".into(),
            error: e.into_kind(),
        })?;

    let mut finder = EmbedPayloadFinder::new();
    module.visit_with(&mut finder);

    Ok(finder.results)
}

pub fn parse_embed_payload_from_file(path: impl AsRef<Path>) -> Result<Vec<PackedCall>> {
    let cm: Lrc<SourceMap> = Default::default();
    let fm = cm
        .load_file(path.as_ref())
        .map_err(|_| ParserError::LoadError)?;
    let lexer = Lexer::new(
        Syntax::Es(Default::default()),
        Default::default(),
        StringInput::from(&*fm),
        None,
    );
    let mut parser = Parser::new_from(lexer);
    let module = parser
        .parse_module()
        .map_err(|e| ParserError::SyntaxError {
            context: "parse embed payload from file".into(),
            error: e.into_kind(),
        })?;

    let mut finder = EmbedPayloadFinder::new();
    module.visit_with(&mut finder);

    Ok(finder.results)
}

pub fn parse_variables(source: impl AsRef<str>) -> Result<Vec<Variable>> {
    let cm: Lrc<SourceMap> = Default::default();
    let fm = cm.new_source_file(
        FileName::Custom("uwu.js".into()).into(),
        source.as_ref().to_string(),
    );
    let lexer = Lexer::new(
        Syntax::Es(Default::default()),
        Default::default(),
        StringInput::from(&*fm),
        None,
    );
    let mut parser = Parser::new_from(lexer);
    let module = parser
        .parse_module()
        .map_err(|e| ParserError::SyntaxError {
            context: "parse embed payload from file".into(),
            error: e.into_kind(),
        })?;

    let mut finder = VariableFinder::new();
    module.visit_with(&mut finder);

    Ok(finder.results)
}
