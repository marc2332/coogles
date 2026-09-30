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
    let mut offset = 0;
    let mut line = 0;
    for token in ra_ap_rustc_lexer::tokenize(source, ra_ap_rustc_lexer::FrontmatterAllowed::No) {
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
        Err(error) => analysis.parse_error = Some(error.to_string()),
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

struct TestVisitor {
    test_lines: Vec<bool>,
    module_path: Vec<String>,
    test_context: bool,
    external_modules: Vec<ExternalModule>,
}

impl TestVisitor {
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
        let previous = self.test_context;
        self.test_context |= test_only(item_attributes(item));
        if self.test_context {
            self.mark(item.span());
        }
        syn::visit::visit_item(self, item);
        self.test_context = previous;
    }

    fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
        if function.attrs.iter().any(|attribute| {
            attribute
                .path()
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "test" || segment.ident == "rstest")
        }) {
            self.mark(function.span());
        }
        syn::visit::visit_item_fn(self, function);
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
    fn reports_parse_errors_but_preserves_lexical_counts() {
        let result = analyze("// comment\nfn broken(\n");
        assert!(result.parse_error.is_some());
        assert_eq!(result.comment_loc, 1);
        assert_eq!(result.code_loc, 1);
    }
}
