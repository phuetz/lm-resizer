//! Résumé syntaxique local. Les positions proviennent des parseurs, jamais
//! d'une recherche de déclarations dans les commentaires ou les chaînes.
use std::path::Path;
use syn::spanned::Spanned;
use tree_sitter::Node;

#[derive(Debug)]
pub enum Fallback {
    UnknownLanguage,
    InvalidSyntax,
}

impl Fallback {
    pub fn marker(&self) -> &'static str {
        match self {
            Self::UnknownLanguage => "[smart AST: repli langue inconnue]\n",
            Self::InvalidSyntax => "[smart AST: repli erreur d'analyse]\n",
        }
    }
}

pub fn summarize(path: &Path, source: &str) -> Result<String, Fallback> {
    // syn retire le BOM avant de calculer les spans : utiliser la même vue
    // pour extraire les signatures, sans modifier les numéros de ligne.
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    let (language, rows) = match extension.to_ascii_lowercase().as_str() {
        "rs" => ("rust", rust(source)?),
        "py" | "pyi" => (
            "python",
            syntax(source, tree_sitter_python::LANGUAGE.into(), true)?,
        ),
        "ts" | "mts" | "cts" => (
            "typescript",
            syntax(
                source,
                tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                false,
            )?,
        ),
        "tsx" => (
            "typescript",
            syntax(source, tree_sitter_typescript::LANGUAGE_TSX.into(), false)?,
        ),
        "js" | "jsx" | "mjs" | "cjs" => (
            "javascript",
            syntax(source, tree_sitter_javascript::LANGUAGE.into(), false)?,
        ),
        _ => return Err(Fallback::UnknownLanguage),
    };
    Ok(format!(
        "[smart AST: {language}; corps omis]\n{}",
        rows.output
    ))
}

struct Rows {
    output: String,
    line_starts: Vec<usize>,
}

fn compact(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

impl Rows {
    fn new(source: &str) -> Self {
        Self {
            output: String::new(),
            line_starts: std::iter::once(0)
                .chain(
                    source
                        .bytes()
                        .enumerate()
                        .filter_map(|(n, b)| (b == b'\n').then_some(n + 1)),
                )
                .collect(),
        }
    }

    fn line(&self, start: usize) -> usize {
        self.line_starts.partition_point(|offset| *offset <= start)
    }

    fn push(&mut self, line: usize, kind: &str, text: &str, doc: Option<String>) {
        self.output
            .push_str(&format!("L{line} {kind} {}", compact(text)));
        if let Some(doc) = doc.filter(|s| !s.is_empty()) {
            self.output.push_str(" // ");
            self.output.push_str(&doc);
        }
        self.output.push('\n');
    }
}

// Les commentaires contigus précédant la déclaration sont de la documentation
// de présentation. Ils n'autorisent jamais l'extraction d'un symbole.
// `#` n'ouvre un commentaire qu'en Python : en Rust c'est un attribut, en
// JavaScript et TypeScript un champ privé.
fn preceding_comment(source: &str, start: usize, hash: bool) -> Option<String> {
    let before = source.get(..start)?;
    let line_start = before.rfind('\n').map_or(0, |n| n + 1);
    if !matches!(
        before[line_start..].trim(),
        "" | "export" | "export default" | "export declare" | "declare"
    ) {
        return None;
    }
    let mut comments = Vec::new();
    for line in before[..line_start].lines().rev() {
        let line = line.trim();
        if line.starts_with(['/', '*']) || (hash && line.starts_with('#')) {
            comments.push(line);
        } else {
            break;
        }
    }
    comments
        .iter()
        .rev()
        .map(|s| clean_comment(s))
        .find(|s| !s.is_empty())
}

fn clean_comment(text: &str) -> String {
    text.lines()
        .map(|line| {
            line.trim()
                .trim_start_matches(['/', '*', '#', '!'])
                .trim()
                .trim_end_matches("*/")
                .trim()
        })
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .to_string()
}

fn rust(source: &str) -> Result<Rows, Fallback> {
    let mut file = syn::parse_file(source).map_err(|_| Fallback::InvalidSyntax)?;
    if let Some(shebang) = &file.shebang {
        // syn retire également le shebang. Le remplacer par autant d'octets
        // d'espaces préserve les spans absolus dans le fichier d'origine.
        let padded = " ".repeat(shebang.len()) + &source[shebang.len()..];
        file = syn::parse_file(&padded).map_err(|_| Fallback::InvalidSyntax)?;
    }
    let mut rows = Rows::new(source);
    if let Some(attr) = file.attrs.iter().find(|a| a.path().is_ident("doc")) {
        if let Some(doc) = rust_doc(std::slice::from_ref(attr)) {
            rows.push(rows.line(attr.span().byte_range().start), "doc", &doc, None);
        }
    }
    rust_items(source, &file.items, &mut rows);
    Ok(rows)
}

fn rust_doc(attrs: &[syn::Attribute]) -> Option<String> {
    attrs.iter().find_map(|a| {
        if !a.path().is_ident("doc") {
            return None;
        }
        if let syn::Meta::NameValue(meta) = &a.meta {
            if let syn::Expr::Lit(value) = &meta.value {
                if let syn::Lit::Str(value) = &value.lit {
                    return value
                        .value()
                        .lines()
                        .find(|line| !line.trim().is_empty())
                        .map(|line| line.trim().to_string());
                }
            }
        }
        None
    })
}

fn rust_start(source: &str, span: proc_macro2::Span, attrs: &[syn::Attribute]) -> usize {
    let start = attrs
        .last()
        .map_or(span.byte_range().start, |a| a.span().byte_range().end);
    start + source[start..].len() - source[start..].trim_start().len()
}

fn rust_row(
    source: &str,
    rows: &mut Rows,
    kind: &str,
    span: proc_macro2::Span,
    attrs: &[syn::Attribute],
    end: usize,
    body: Option<usize>,
) {
    let start = rust_start(source, span, attrs);
    // Les commentaires se lisent au-dessus des attributs, pas entre eux et
    // la déclaration.
    let first = attrs
        .iter()
        .map(|a| a.span().byte_range().start)
        .min()
        .unwrap_or(start);
    let doc = rust_doc(attrs)
        .or_else(|| preceding_comment(source, first, false))
        .or_else(|| {
            let text = source.get(body?..)?.trim_start();
            text.starts_with("//").then(|| clean_comment(text))
        });
    rows.push(rows.line(start), kind, &source[start..end], doc);
}

fn rust_fields(source: &str, rows: &mut Rows, fields: &syn::Fields) {
    for field in fields {
        rust_row(
            source,
            rows,
            "field",
            field.span(),
            &field.attrs,
            field.span().byte_range().end,
            None,
        );
    }
}

fn rust_items(source: &str, items: &[syn::Item], rows: &mut Rows) {
    for item in items {
        match item {
            syn::Item::Use(i) => rust_row(
                source,
                rows,
                "import",
                i.span(),
                &i.attrs,
                i.span().byte_range().end,
                None,
            ),
            syn::Item::ExternCrate(i) => rust_row(
                source,
                rows,
                "import",
                i.span(),
                &i.attrs,
                i.span().byte_range().end,
                None,
            ),
            syn::Item::ForeignMod(i) => {
                rust_row(
                    source,
                    rows,
                    "extern",
                    i.span(),
                    &i.attrs,
                    i.brace_token.span.open().byte_range().start,
                    None,
                );
                for member in &i.items {
                    match member {
                        syn::ForeignItem::Fn(f) => rust_row(
                            source,
                            rows,
                            "fn",
                            f.span(),
                            &f.attrs,
                            f.span().byte_range().end,
                            None,
                        ),
                        syn::ForeignItem::Static(s)
                            if !matches!(s.vis, syn::Visibility::Inherited) =>
                        {
                            rust_row(
                                source,
                                rows,
                                "static",
                                s.span(),
                                &s.attrs,
                                s.span().byte_range().end,
                                None,
                            )
                        }
                        syn::ForeignItem::Type(t) => rust_row(
                            source,
                            rows,
                            "type",
                            t.span(),
                            &t.attrs,
                            t.span().byte_range().end,
                            None,
                        ),
                        _ => {}
                    }
                }
            }
            syn::Item::Fn(i) => rust_row(
                source,
                rows,
                "fn",
                i.span(),
                &i.attrs,
                i.block.brace_token.span.open().byte_range().start,
                Some(i.block.brace_token.span.open().byte_range().end),
            ),
            syn::Item::Struct(i) => {
                let end = match &i.fields {
                    syn::Fields::Named(f) => f.brace_token.span.open().byte_range().start,
                    syn::Fields::Unnamed(f) => f.paren_token.span.open().byte_range().start,
                    syn::Fields::Unit => i.span().byte_range().end,
                };
                rust_row(source, rows, "struct", i.span(), &i.attrs, end, None);
                rust_fields(source, rows, &i.fields);
            }
            syn::Item::Union(i) => {
                rust_row(
                    source,
                    rows,
                    "union",
                    i.span(),
                    &i.attrs,
                    i.fields.brace_token.span.open().byte_range().start,
                    None,
                );
                for f in &i.fields.named {
                    rust_row(
                        source,
                        rows,
                        "field",
                        f.span(),
                        &f.attrs,
                        f.span().byte_range().end,
                        None,
                    );
                }
            }
            syn::Item::Enum(i) => {
                rust_row(
                    source,
                    rows,
                    "enum",
                    i.span(),
                    &i.attrs,
                    i.brace_token.span.open().byte_range().start,
                    None,
                );
                for v in &i.variants {
                    let end = v
                        .discriminant
                        .as_ref()
                        .map_or(v.span().byte_range().end, |(eq, _)| {
                            eq.span().byte_range().start
                        });
                    rust_row(source, rows, "variant", v.span(), &v.attrs, end, None);
                }
            }
            syn::Item::Type(i) => rust_row(
                source,
                rows,
                "type",
                i.span(),
                &i.attrs,
                i.span().byte_range().end,
                None,
            ),
            syn::Item::Const(i) if !matches!(i.vis, syn::Visibility::Inherited) => rust_row(
                source,
                rows,
                "const",
                i.span(),
                &i.attrs,
                i.eq_token.span().byte_range().start,
                None,
            ),
            syn::Item::Static(i) if !matches!(i.vis, syn::Visibility::Inherited) => rust_row(
                source,
                rows,
                "static",
                i.span(),
                &i.attrs,
                i.eq_token.span().byte_range().start,
                None,
            ),
            syn::Item::Mod(i) => {
                let end = i
                    .content
                    .as_ref()
                    .map_or(i.span().byte_range().end, |(brace, _)| {
                        brace.span.open().byte_range().start
                    });
                rust_row(source, rows, "mod", i.span(), &i.attrs, end, None);
                if let Some((_, items)) = &i.content {
                    rust_items(source, items, rows);
                }
            }
            syn::Item::Impl(i) => {
                rust_row(
                    source,
                    rows,
                    "impl",
                    i.span(),
                    &i.attrs,
                    i.brace_token.span.open().byte_range().start,
                    None,
                );
                for member in &i.items {
                    match member {
                        syn::ImplItem::Fn(f) => rust_row(
                            source,
                            rows,
                            "method",
                            f.span(),
                            &f.attrs,
                            f.block.brace_token.span.open().byte_range().start,
                            Some(f.block.brace_token.span.open().byte_range().end),
                        ),
                        syn::ImplItem::Const(c) if !matches!(c.vis, syn::Visibility::Inherited) => {
                            rust_row(
                                source,
                                rows,
                                "const",
                                c.span(),
                                &c.attrs,
                                c.eq_token.span().byte_range().start,
                                None,
                            )
                        }
                        syn::ImplItem::Type(t) => rust_row(
                            source,
                            rows,
                            "type",
                            t.span(),
                            &t.attrs,
                            t.span().byte_range().end,
                            None,
                        ),
                        _ => {}
                    }
                }
            }
            syn::Item::Trait(i) => {
                rust_row(
                    source,
                    rows,
                    "trait",
                    i.span(),
                    &i.attrs,
                    i.brace_token.span.open().byte_range().start,
                    None,
                );
                for member in &i.items {
                    match member {
                        syn::TraitItem::Fn(f) => {
                            let end = f.default.as_ref().map_or(f.span().byte_range().end, |b| {
                                b.brace_token.span.open().byte_range().start
                            });
                            rust_row(source, rows, "method", f.span(), &f.attrs, end, None);
                        }
                        syn::TraitItem::Const(c) => rust_row(
                            source,
                            rows,
                            "const",
                            c.span(),
                            &c.attrs,
                            c.default
                                .as_ref()
                                .map_or(c.span().byte_range().end, |(eq, _)| {
                                    eq.span().byte_range().start
                                }),
                            None,
                        ),
                        syn::TraitItem::Type(t) => rust_row(
                            source,
                            rows,
                            "type",
                            t.span(),
                            &t.attrs,
                            t.span().byte_range().end,
                            None,
                        ),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

fn syntax(source: &str, language: tree_sitter::Language, python: bool) -> Result<Rows, Fallback> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .map_err(|_| Fallback::InvalidSyntax)?;
    let tree = parser.parse(source, None).ok_or(Fallback::InvalidSyntax)?;
    if tree.root_node().has_error() {
        return Err(Fallback::InvalidSyntax);
    }
    let mut rows = Rows::new(source);
    walk(source, tree.root_node(), &mut rows, python, false);
    Ok(rows)
}

fn text<'a>(source: &'a str, node: Node<'_>) -> &'a str {
    &source[node.byte_range()]
}

fn docstring(source: &str, body: Node<'_>) -> Option<(usize, String)> {
    let mut cursor = body.walk();
    let first = body
        .named_children(&mut cursor)
        .find(|n| n.kind() != "comment")?;
    if first.kind() != "expression_statement" {
        return None;
    }
    let string = first.named_child(0)?;
    if string.kind() != "string" {
        return None;
    }
    let mut cursor = string.walk();
    let content = string
        .named_children(&mut cursor)
        .find(|n| n.kind() == "string_content")?;
    let (offset, line) = text(source, content)
        .lines()
        .enumerate()
        .find(|(_, line)| !line.trim().is_empty())?;
    Some((
        content.start_position().row + offset + 1,
        line.trim().to_string(),
    ))
}

fn tree_row(
    source: &str,
    node: Node<'_>,
    rows: &mut Rows,
    kind: &str,
    start: usize,
    end: usize,
    python: bool,
) {
    let doc = if python {
        node.child_by_field_name("body")
            .and_then(|b| docstring(source, b))
            .map(|(_, doc)| doc)
    } else {
        None
    };
    rows.push(
        rows.line(start),
        kind,
        &source[start..end],
        doc.or_else(|| preceding_comment(source, start, python)),
    );
}

fn python_class_member(node: Node<'_>) -> bool {
    let outer = if node
        .parent()
        .is_some_and(|p| p.kind() == "decorated_definition")
    {
        node.parent().unwrap()
    } else {
        node
    };
    outer
        .parent()
        .filter(|p| p.kind() == "block")
        .and_then(|p| p.parent())
        .is_some_and(|p| p.kind() == "class_definition")
}

fn callable(value: Node<'_>) -> bool {
    matches!(
        value.kind(),
        "arrow_function" | "function_expression" | "generator_function"
    )
}

fn walk(source: &str, node: Node<'_>, rows: &mut Rows, python: bool, exported: bool) {
    let start = node.start_byte();
    let end = node.end_byte();
    match node.kind() {
        "module" | "program" | "block" | "class_body" | "interface_body" | "object_type"
        | "enum_body" | "statement_block" => {
            if python && node.kind() == "module" {
                if let Some((line, doc)) = docstring(source, node) {
                    rows.push(line, "doc", &doc, None);
                }
            }
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                walk(source, child, rows, python, exported);
            }
        }
        "export_statement" => {
            if let Some(declaration) = node.child_by_field_name("declaration") {
                let position = rows.output.len();
                walk(source, declaration, rows, python, true);
                // Conserver le préfixe export du premier symbole sans changer
                // la position des membres suivants.
                if rows.output.len() > position {
                    let line = declaration.start_position().row + 1;
                    let prefix = compact(&source[start..declaration.start_byte()]);
                    let old = format!("L{line} ");
                    if let Some(rest) = rows.output[position..].strip_prefix(&old) {
                        if let Some((kind, _)) = rest.split_once(' ') {
                            let insert = position + old.len() + kind.len() + 1;
                            rows.output.insert_str(insert, &format!("{prefix} "));
                        }
                    }
                }
            } else {
                tree_row(source, node, rows, "export", start, end, python);
            }
        }
        "decorated_definition" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                walk(source, child, rows, python, exported);
            }
        }
        "decorator" => tree_row(source, node, rows, "decorator", start, end, python),
        "import_statement" | "import_from_statement" | "future_import_statement" => {
            tree_row(source, node, rows, "import", start, end, python)
        }
        "function_definition"
        | "function_declaration"
        | "generator_function_declaration"
        | "method_definition"
        | "function_signature"
        | "method_signature"
        | "abstract_method_signature" => {
            let body = node.child_by_field_name("body");
            let end = body.map_or(end, |b| b.start_byte());
            let kind = if node.kind().contains("method") || (python && python_class_member(node)) {
                "method"
            } else {
                "fn"
            };
            tree_row(source, node, rows, kind, start, end, python);
        }
        "class_definition"
        | "class_declaration"
        | "abstract_class_declaration"
        | "interface_declaration"
        | "enum_declaration"
        | "internal_module" => {
            let body = node.child_by_field_name("body");
            tree_row(
                source,
                node,
                rows,
                "type",
                start,
                body.map_or(end, |b| b.start_byte()),
                python,
            );
            if let Some(body) = body {
                walk(source, body, rows, python, exported);
            }
        }
        "type_alias_declaration" => {
            let value = node.child_by_field_name("value");
            if let Some(value) = value.filter(|n| n.kind() == "object_type") {
                tree_row(
                    source,
                    node,
                    rows,
                    "type",
                    start,
                    value.start_byte(),
                    python,
                );
                walk(source, value, rows, python, exported);
            } else {
                tree_row(source, node, rows, "type", start, end, python);
            }
        }
        "type_alias_statement" if python => {
            tree_row(source, node, rows, "type", start, end, python)
        }
        "lexical_declaration" | "variable_declaration" => {
            let mut cursor = node.walk();
            for child in node
                .named_children(&mut cursor)
                .filter(|n| n.kind() == "variable_declarator")
            {
                let value = child.child_by_field_name("value");
                let callable = value.is_some_and(callable);
                if callable || exported {
                    let prefix = source[start
                        ..node
                            .named_child(0)
                            .map_or(child.start_byte(), |n| n.start_byte())]
                        .trim();
                    let until = if callable {
                        value
                            .and_then(|n| n.child_by_field_name("body"))
                            .map_or(child.end_byte(), |n| n.start_byte())
                    } else {
                        value.map_or(child.end_byte(), |n| n.start_byte())
                    };
                    let signature = source[child.start_byte()..until]
                        .trim_end()
                        .trim_end_matches('=')
                        .trim_end();
                    rows.push(
                        child.start_position().row + 1,
                        if callable { "fn" } else { "const" },
                        &format!("{prefix} {signature}"),
                        preceding_comment(source, start, python),
                    );
                }
            }
        }
        "public_field_definition"
        | "field_definition"
        | "property_signature"
        | "enum_assignment" => {
            let value = node.child_by_field_name("value");
            let callable = value.is_some_and(callable);
            let end = if callable {
                value
                    .and_then(|n| n.child_by_field_name("body"))
                    .map_or(end, |n| n.start_byte())
            } else {
                value.map_or(end, |n| n.start_byte())
            };
            let signature = source[start..end]
                .trim_end()
                .trim_end_matches('=')
                .trim_end();
            rows.push(
                node.start_position().row + 1,
                if callable { "method" } else { "field" },
                signature,
                preceding_comment(source, start, python),
            );
        }
        "property_identifier" if node.parent().is_some_and(|n| n.kind() == "enum_body") => {
            tree_row(source, node, rows, "variant", start, end, python)
        }
        "expression_statement" if python => {
            if let Some(assignment) = node.named_child(0).filter(|n| n.kind() == "assignment") {
                if let Some(left) = assignment.child_by_field_name("left") {
                    let name = text(source, left);
                    let field = python_class_member(node)
                        && assignment.child_by_field_name("type").is_some();
                    if field
                        || (name.chars().all(|c| !c.is_ascii_lowercase())
                            && name.chars().any(|c| c.is_ascii_uppercase()))
                    {
                        let end = assignment
                            .child_by_field_name("right")
                            .map_or(assignment.end_byte(), |n| n.start_byte());
                        let signature = source[assignment.start_byte()..end]
                            .trim_end()
                            .trim_end_matches('=')
                            .trim_end();
                        rows.push(
                            node.start_position().row + 1,
                            if field { "field" } else { "const" },
                            signature,
                            preceding_comment(source, start, python),
                        );
                    }
                }
            }
        }
        _ => {}
    }
}
