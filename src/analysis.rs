use proc_macro2::Span;
use ra_ap_rustc_lexer::{DocStyle, TokenKind};
use syn::{Attribute, Item, Meta, Token, punctuated::Punctuated, spanned::Spanned, visit::Visit};

#[derive(Default)]
pub struct Analysis {
    pub code_loc: u64,
    pub inline_test_loc: u64,
    pub comment_loc: u64,
    pub inner_doc_loc: u64,
    pub outer_doc_loc: u64,
    pub block_comment_loc: u64,
    pub block_doc_loc: u64,
    pub parse_error: Option<String>,
    pub external_modules: Vec<ExternalModule>,
}

pub struct ExternalModule {
    pub path: String,
    pub from_file_parent: bool,
    pub test_only: bool,
}

pub fn analyze(source: &str) -> Analysis {
    let lines: Vec<&str> = source.lines().collect();
    let mut code = vec![false; lines.len()];
    let mut comments = code.clone();
    let mut inner_docs = code.clone();
    let mut outer_docs = code.clone();
    let mut block_comments = code.clone();
    let mut block_docs = code.clone();
    let mut offset = usize::from(source.starts_with('\u{feff}')) * '\u{feff}'.len_utf8();
    offset += ra_ap_rustc_lexer::strip_shebang(&source[offset..]).unwrap_or_default();
    let mut line = 0;
    for token in
        ra_ap_rustc_lexer::tokenize(&source[offset..], ra_ap_rustc_lexer::FrontmatterAllowed::No)
    {
        let end = offset + token.len as usize;
        let text = &source[offset..end];
        let last_line = line + text.bytes().filter(|byte| *byte == b'\n').count();
        let inclusive_end = if text.ends_with('\n') {
            last_line.saturating_sub(1)
        } else {
            last_line
        };
        let destination = match token.kind {
            TokenKind::Whitespace => None,
            TokenKind::LineComment { doc_style: None } => Some(&mut comments),
            TokenKind::LineComment {
                doc_style: Some(DocStyle::Inner),
            } => Some(&mut inner_docs),
            TokenKind::LineComment {
                doc_style: Some(DocStyle::Outer),
            } => Some(&mut outer_docs),
            TokenKind::BlockComment {
                doc_style: None, ..
            } => Some(&mut block_comments),
            TokenKind::BlockComment { .. } => Some(&mut block_docs),
            _ => Some(&mut code),
        };
        if let Some(destination) = destination {
            for (value, source_line) in destination
                .iter_mut()
                .zip(&lines)
                .take(inclusive_end + 1)
                .skip(line)
            {
                *value = !source_line.trim().is_empty();
            }
        }
        line = last_line;
        offset = end;
    }
    let count = |values: &[bool]| values.iter().filter(|value| **value).count() as u64;
    let mut analysis = Analysis {
        code_loc: count(&code),
        comment_loc: count(&comments),
        inner_doc_loc: count(&inner_docs),
        outer_doc_loc: count(&outer_docs),
        block_comment_loc: count(&block_comments),
        block_doc_loc: count(&block_docs),
        ..Analysis::default()
    };
    match syn::parse_file(source) {
        Ok(file) => {
            let mut visitor = TestVisitor {
                test_lines: vec![test_only(&file.attrs); lines.len()],
                module_path: Vec::new(),
                test_context: test_only(&file.attrs),
                external_modules: Vec::new(),
            };
            visitor.visit_file(&file);
            analysis.inline_test_loc = code
                .iter()
                .zip(visitor.test_lines)
                .filter(|(is_code, is_test)| **is_code && *is_test)
                .count() as u64;
            analysis.external_modules = visitor.external_modules;
        }
        Err(error) => {
            let location = error.span().start();
            analysis.parse_error = Some(format!(
                "{error} at {}:{}",
                location.line,
                location.column + 1
            ));
        }
    }
    analysis
}

fn requires_test(meta: &Meta) -> bool {
    match meta {
        Meta::Path(path) => path.is_ident("test"),
        Meta::List(list) => {
            let Ok(children) =
                list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
            else {
                return false;
            };
            if list.path.is_ident("all") {
                children.iter().any(requires_test)
            } else if list.path.is_ident("any") {
                !children.is_empty() && children.iter().all(requires_test)
            } else {
                false
            }
        }
        _ => false,
    }
}

fn test_only(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute.path().is_ident("cfg")
            && attribute
                .parse_args::<Meta>()
                .is_ok_and(|meta| requires_test(&meta))
    })
}

fn item_attributes(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::ExternCrate(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::ForeignMod(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::TraitAlias(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Union(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        _ => &[],
    }
}

fn expression_attributes(expression: &syn::Expr) -> &[Attribute] {
    match expression {
        syn::Expr::Array(expression) => &expression.attrs,
        syn::Expr::Assign(expression) => &expression.attrs,
        syn::Expr::Async(expression) => &expression.attrs,
        syn::Expr::Await(expression) => &expression.attrs,
        syn::Expr::Binary(expression) => &expression.attrs,
        syn::Expr::Block(expression) => &expression.attrs,
        syn::Expr::Break(expression) => &expression.attrs,
        syn::Expr::Call(expression) => &expression.attrs,
        syn::Expr::Cast(expression) => &expression.attrs,
        syn::Expr::Closure(expression) => &expression.attrs,
        syn::Expr::Const(expression) => &expression.attrs,
        syn::Expr::Continue(expression) => &expression.attrs,
        syn::Expr::Field(expression) => &expression.attrs,
        syn::Expr::ForLoop(expression) => &expression.attrs,
        syn::Expr::Group(expression) => &expression.attrs,
        syn::Expr::If(expression) => &expression.attrs,
        syn::Expr::Index(expression) => &expression.attrs,
        syn::Expr::Infer(expression) => &expression.attrs,
        syn::Expr::Let(expression) => &expression.attrs,
        syn::Expr::Lit(expression) => &expression.attrs,
        syn::Expr::Loop(expression) => &expression.attrs,
        syn::Expr::Macro(expression) => &expression.attrs,
        syn::Expr::Match(expression) => &expression.attrs,
        syn::Expr::MethodCall(expression) => &expression.attrs,
        syn::Expr::Paren(expression) => &expression.attrs,
        syn::Expr::Path(expression) => &expression.attrs,
        syn::Expr::Range(expression) => &expression.attrs,
        syn::Expr::RawAddr(expression) => &expression.attrs,
        syn::Expr::Reference(expression) => &expression.attrs,
        syn::Expr::Repeat(expression) => &expression.attrs,
        syn::Expr::Return(expression) => &expression.attrs,
        syn::Expr::Struct(expression) => &expression.attrs,
        syn::Expr::Try(expression) => &expression.attrs,
        syn::Expr::TryBlock(expression) => &expression.attrs,
        syn::Expr::Tuple(expression) => &expression.attrs,
        syn::Expr::Unary(expression) => &expression.attrs,
        syn::Expr::Unsafe(expression) => &expression.attrs,
        syn::Expr::While(expression) => &expression.attrs,
        syn::Expr::Yield(expression) => &expression.attrs,
        _ => &[],
    }
}

struct TestVisitor {
    test_lines: Vec<bool>,
    module_path: Vec<String>,
    test_context: bool,
    external_modules: Vec<ExternalModule>,
}

impl TestVisitor {
    fn enter_region(&mut self, attributes: &[Attribute], span: Span) -> bool {
        let previous = self.test_context;
        self.test_context |= test_only(attributes);
        if self.test_context && !previous {
            self.mark(span);
        }
        previous
    }

    fn mark(&mut self, span: Span) {
        let start = span.start().line.saturating_sub(1);
        let end = span.end().line.min(self.test_lines.len());
        if let Some(lines) = self.test_lines.get_mut(start..end) {
            lines.fill(true);
        }
    }
}

impl<'ast> Visit<'ast> for TestVisitor {
    fn visit_item(&mut self, item: &'ast Item) {
        let previous = self.enter_region(item_attributes(item), item.span());
        syn::visit::visit_item(self, item);
        self.test_context = previous;
    }

    fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
        let previous = self.test_context;
        self.test_context |= function.attrs.iter().any(|attribute| {
            attribute
                .path()
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "test" || segment.ident == "rstest")
        });
        if self.test_context && !previous {
            self.mark(function.span());
        }
        syn::visit::visit_item_fn(self, function);
        self.test_context = previous;
    }

    fn visit_impl_item(&mut self, item: &'ast syn::ImplItem) {
        let attributes = match item {
            syn::ImplItem::Const(item) => &item.attrs,
            syn::ImplItem::Fn(item) => &item.attrs,
            syn::ImplItem::Type(item) => &item.attrs,
            syn::ImplItem::Macro(item) => &item.attrs,
            _ => return syn::visit::visit_impl_item(self, item),
        };
        let previous = self.enter_region(attributes, item.span());
        syn::visit::visit_impl_item(self, item);
        self.test_context = previous;
    }

    fn visit_trait_item(&mut self, item: &'ast syn::TraitItem) {
        let attributes = match item {
            syn::TraitItem::Const(item) => &item.attrs,
            syn::TraitItem::Fn(item) => &item.attrs,
            syn::TraitItem::Type(item) => &item.attrs,
            syn::TraitItem::Macro(item) => &item.attrs,
            _ => return syn::visit::visit_trait_item(self, item),
        };
        let previous = self.enter_region(attributes, item.span());
        syn::visit::visit_trait_item(self, item);
        self.test_context = previous;
    }

    fn visit_foreign_item(&mut self, item: &'ast syn::ForeignItem) {
        let attributes = match item {
            syn::ForeignItem::Fn(item) => &item.attrs,
            syn::ForeignItem::Static(item) => &item.attrs,
            syn::ForeignItem::Type(item) => &item.attrs,
            syn::ForeignItem::Macro(item) => &item.attrs,
            _ => return syn::visit::visit_foreign_item(self, item),
        };
        let previous = self.enter_region(attributes, item.span());
        syn::visit::visit_foreign_item(self, item);
        self.test_context = previous;
    }

    fn visit_field(&mut self, field: &'ast syn::Field) {
        let previous = self.enter_region(&field.attrs, field.span());
        syn::visit::visit_field(self, field);
        self.test_context = previous;
    }

    fn visit_variant(&mut self, variant: &'ast syn::Variant) {
        let previous = self.enter_region(&variant.attrs, variant.span());
        syn::visit::visit_variant(self, variant);
        self.test_context = previous;
    }

    fn visit_local(&mut self, local: &'ast syn::Local) {
        let previous = self.enter_region(&local.attrs, local.span());
        syn::visit::visit_local(self, local);
        self.test_context = previous;
    }

    fn visit_stmt_macro(&mut self, statement: &'ast syn::StmtMacro) {
        let previous = self.enter_region(&statement.attrs, statement.span());
        syn::visit::visit_stmt_macro(self, statement);
        self.test_context = previous;
    }

    fn visit_pat_type(&mut self, pattern: &'ast syn::PatType) {
        let previous = self.enter_region(&pattern.attrs, pattern.span());
        syn::visit::visit_pat_type(self, pattern);
        self.test_context = previous;
    }

    fn visit_receiver(&mut self, receiver: &'ast syn::Receiver) {
        let previous = self.enter_region(&receiver.attrs, receiver.span());
        syn::visit::visit_receiver(self, receiver);
        self.test_context = previous;
    }

    fn visit_generic_param(&mut self, parameter: &'ast syn::GenericParam) {
        let attributes = match parameter {
            syn::GenericParam::Lifetime(parameter) => &parameter.attrs,
            syn::GenericParam::Type(parameter) => &parameter.attrs,
            syn::GenericParam::Const(parameter) => &parameter.attrs,
        };
        let previous = self.enter_region(attributes, parameter.span());
        syn::visit::visit_generic_param(self, parameter);
        self.test_context = previous;
    }

    fn visit_expr(&mut self, expression: &'ast syn::Expr) {
        let previous = self.enter_region(expression_attributes(expression), expression.span());
        syn::visit::visit_expr(self, expression);
        self.test_context = previous;
    }

    fn visit_arm(&mut self, arm: &'ast syn::Arm) {
        let previous = self.enter_region(&arm.attrs, arm.span());
        syn::visit::visit_arm(self, arm);
        self.test_context = previous;
    }

    fn visit_field_value(&mut self, field: &'ast syn::FieldValue) {
        let previous = self.enter_region(&field.attrs, field.span());
        syn::visit::visit_field_value(self, field);
        self.test_context = previous;
    }

    fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
        if module.content.is_none() {
            let explicit_path = module.attrs.iter().find_map(|attribute| {
                if !attribute.path().is_ident("path") {
                    return None;
                }
                let Meta::NameValue(value) = &attribute.meta else {
                    return None;
                };
                let syn::Expr::Lit(value) = &value.value else {
                    return None;
                };
                let syn::Lit::Str(value) = &value.lit else {
                    return None;
                };
                Some(value.value())
            });
            let prefix = if self.module_path.is_empty() {
                String::new()
            } else {
                format!("{}/", self.module_path.join("/"))
            };
            let from_file_parent = explicit_path.is_some() && self.module_path.is_empty();
            let paths = if let Some(path) = explicit_path {
                vec![format!("{prefix}{path}")]
            } else {
                vec![
                    format!("{prefix}{}.rs", module.ident),
                    format!("{prefix}{}/mod.rs", module.ident),
                ]
            };
            for path in paths {
                self.external_modules.push(ExternalModule {
                    path,
                    from_file_parent,
                    test_only: self.test_context,
                });
            }
        }
        self.module_path.push(module.ident.to_string());
        syn::visit::visit_item_mod(self, module);
        self.module_path.pop();
    }
}

#[cfg(test)]
mod tests {
    use crate::analysis::analyze;

    #[test]
    fn distinguishes_comments_from_literals_and_docs() {
        let result = analyze(
            "//! inner\n/// outer\n//// regular\nfn main() { let text = r#\"// not a comment\"#; } // trailing\n/* block */\n",
        );
        assert_eq!(result.comment_loc, 2);
        assert_eq!(result.inner_doc_loc, 1);
        assert_eq!(result.outer_doc_loc, 1);
        assert_eq!(result.block_comment_loc, 1);
        assert_eq!(result.code_loc, 1);
    }

    #[test]
    fn counts_inline_tests_without_double_counting() {
        let result = analyze(
            "fn production() {}\n#[cfg(all(test, feature = \"foo\"))]\nmod checks {\n // explanation\n #[tokio::test]\n async fn works() {}\n mod helpers;\n}\n#[cfg(any(test, feature = \"foo\"))]\nfn also_production() {}\n",
        );
        assert_eq!(result.inline_test_loc, 6);
        assert_eq!(
            result
                .external_modules
                .iter()
                .filter(|module| module.test_only)
                .map(|module| module.path.as_str())
                .collect::<Vec<_>>(),
            ["checks/helpers.rs", "checks/helpers/mod.rs"]
        );
        assert!(result.parse_error.is_none());
    }

    #[test]
    fn counts_multiline_literals_nested_comments_and_crlf() {
        let source = "//! module docs\n/// function docs\n//// ordinary\nfn main() {\n let text = r##\"// literal\n/// literal\n\n/* literal */\"##;\n let url = \"https://example.com\"; // trailing\n /* block\n * nested /* comment */\n\n */ let value = 1;\n /** doc block\n * docs */\n println!(\"{text}\");\n}\n// final";
        for source in [source.to_owned(), source.replace('\n', "\r\n")] {
            let result = analyze(&source);
            assert_eq!(result.code_loc, 8);
            assert_eq!(result.comment_loc, 3);
            assert_eq!(result.inner_doc_loc, 1);
            assert_eq!(result.outer_doc_loc, 1);
            assert_eq!(result.block_comment_loc, 3);
            assert_eq!(result.block_doc_loc, 2);
            assert!(result.parse_error.is_none());
        }
    }

    #[test]
    fn excludes_shebang_and_bom_from_code() {
        for source in [
            "#!/usr/bin/env rust-script\n//! docs\nfn main() {}\n",
            "\u{feff}//! docs\nfn main() {}\n",
        ] {
            let result = analyze(source);
            assert_eq!(result.code_loc, 1);
            assert_eq!(result.inner_doc_loc, 1);
            assert!(result.parse_error.is_none());
        }
    }

    #[test]
    fn detects_test_only_methods_fields_statements_and_associated_items() {
        for source in [
            "struct Thing;\nimpl Thing {\n #[cfg(test)]\n fn helper() {}\n fn normal() {}\n}\n",
            "trait Thing {\n #[cfg(test)]\n const TEST_VALUE: usize;\n}\n",
            "impl Thing {\n #[cfg(test)]\n const TEST_VALUE: usize = 1;\n}\n",
            "unsafe extern \"C\" {\n #[cfg(test)]\n fn helper();\n fn normal();\n}\n",
            "fn main() {\n match thing {\n #[cfg(test)]\n Helper => 1,\n Normal => 2,\n }\n}\n",
            "fn main() {\n let thing = Thing {\n #[cfg(test)]\n helper: 1,\n normal: 2,\n };\n}\n",
            "struct Thing {\n #[cfg(test)]\n helper: usize,\n normal: usize,\n}\n",
            "enum Thing {\n #[cfg(test)]\n Helper,\n Normal,\n}\n",
            "fn main() {\n #[cfg(test)]\n let helper = 1;\n let normal = 2;\n}\n",
            "fn main() {\n #[cfg(test)]\n helper!();\n normal();\n}\n",
            "fn helper(\n #[cfg(test)]\n value: usize,\n normal: usize,\n) {}\n",
            "fn helper<\n #[cfg(test)]\n T,\n>(value: usize) {}\n",
            "impl Thing {\n fn helper(\n #[cfg(test)]\n &self,\n ) {}\n}\n",
        ] {
            let result = analyze(source);
            assert!(result.parse_error.is_none());
            assert_eq!(result.inline_test_loc, 2, "{source}");
        }
        let result = analyze("fn main() {\n #[cfg(test)]\n {\n  check();\n }\n normal();\n}\n");
        assert_eq!(result.inline_test_loc, 4);
    }

    #[test]
    fn external_modules_inside_test_functions_inherit_test_context() {
        let result = analyze("#[test]\nfn check() {\n mod helpers;\n}\n");
        assert!(
            result
                .external_modules
                .iter()
                .all(|module| module.test_only)
        );
    }

    #[test]
    fn reports_parse_errors_but_preserves_lexical_counts() {
        let result = analyze("// comment\nfn broken(\n");
        assert!(result.parse_error.is_some());
        assert_eq!(result.comment_loc, 1);
        assert_eq!(result.code_loc, 1);
    }
}
