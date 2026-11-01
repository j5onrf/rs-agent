use clap::Parser;
use ignore::WalkBuilder;
use quote::ToTokens;
use std::fs;
use std::path::{Path, PathBuf};
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

#[derive(Parser, Debug)]
#[command(author, version, about = "High-signal, token-efficient codebase indexer for AI")]
struct Args {
    #[arg(default_value = ".")]
    path: PathBuf,

    #[arg(short, long)]
    all: bool,

    #[arg(short, long)]
    print: bool,

    #[arg(short, long)]
    output: Option<PathBuf>,
}

fn find_workspace_root(start: &Path) -> PathBuf {
    let mut curr = if start.is_relative() {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(start)
    } else {
        start.to_path_buf()
    };
    curr = fs::canonicalize(&curr).unwrap_or(curr);
    if curr.is_file() {
        curr = curr.parent().unwrap_or(&curr).to_path_buf();
    }
    let mut check = curr.clone();
    loop {
        if check.join("Cargo.toml").is_file() || check.join(".git").exists() {
            return check;
        }
        if let Some(parent) = check.parent() {
            check = parent.to_path_buf();
        } else {
            break;
        }
    }
    curr
}

fn compact_tokens(s: &str) -> String {
    s.replace(" : ", ": ")
     .replace(" , ", ", ")
     .replace(" ;", ";")
     .replace(" < ", "<")
     .replace(" > ", ">")
     .replace(" >", ">")
     .replace("< ", "<")
     .replace(" (", "(")
     .replace("( ", "(")
     .replace(" )", ")")
     .replace("& ", "&")
     .replace(" -> ", " -> ")
}

fn strip_html_tags(input: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in input.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            out.push(c);
        }
    }
    out.replace("&nbsp;", " ")
       .replace("&bull;", "•")
       .replace("•", " ")
       .split_whitespace()
       .collect::<Vec<_>>()
       .join(" ")
}

fn is_visible(vis: &syn::Visibility, show_all: bool) -> bool {
    if show_all {
        return true;
    }
    !matches!(vis, syn::Visibility::Inherited)
}

fn format_vis(vis: &syn::Visibility) -> String {
    let s = vis.to_token_stream().to_string();
    if s.is_empty() { s } else { format!("{} ", s) }
}

fn extract_first_doc(attrs: &[syn::Attribute]) -> Option<String> {
    for attr in attrs {
        if let syn::Meta::NameValue(meta) = &attr.meta {
            if meta.path.is_ident("doc") {
                if let syn::Expr::Lit(expr_lit) = &meta.value {
                    if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                        let doc = lit_str.value().trim().to_string();
                        if !doc.is_empty() {
                            return Some(doc);
                        }
                    }
                }
            }
        }
    }
    None
}

struct RustVisitor {
    show_all: bool,
    is_root_file: bool,
    outline: Vec<String>,
    current_impl: Option<String>,
}

impl<'ast> Visit<'ast> for RustVisitor {
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        let line = node.span().start().line;
        let vis = format_vis(&node.vis);
        self.outline.push(format!("  L{:03}: {}mod {};", line, vis, node.ident));
        visit::visit_item_mod(self, node);
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        let ident_str = node.ident.to_string();
        let is_cli = self.is_root_file && (ident_str == "Cli" || ident_str == "Args" || ident_str == "Opt");

        if is_visible(&node.vis, self.show_all) || is_cli {
            let line = node.span().start().line;
            let doc = extract_first_doc(&node.attrs).map(|d| format!(" // {}", d)).unwrap_or_default();
            let vis = if is_cli && !is_visible(&node.vis, false) { "/* CLI */ ".to_string() } else { format_vis(&node.vis) };

            match &node.fields {
                syn::Fields::Named(fields) => {
                    let field_strs: Vec<String> = fields
                        .named
                        .iter()
                        .filter(|f| is_visible(&f.vis, self.show_all) || is_cli)
                        .map(|f| {
                            let f_vis = format_vis(&f.vis);
                            let f_ident = f.ident.as_ref().map(|i| i.to_string()).unwrap_or_default();
                            let f_ty = compact_tokens(&f.ty.to_token_stream().to_string());
                            format!("{}{}: {}", f_vis, f_ident, f_ty)
                        })
                        .collect();
                    if field_strs.is_empty() {
                        self.outline.push(format!("  L{:03}: {}struct {}{}", line, vis, node.ident, doc));
                    } else if field_strs.len() <= 4 {
                        self.outline.push(format!(
                            "  L{:03}: {}struct {} {{ {} }}{}",
                            line, vis, node.ident, field_strs.join(", "), doc
                        ));
                    } else {
                        self.outline.push(format!(
                            "  L{:03}: {}struct {} {{ {}, ... ({} fields) }}{}",
                            line, vis, node.ident, field_strs[..3].join(", "), field_strs.len(), doc
                        ));
                    }
                }
                syn::Fields::Unnamed(fields) => {
                    let tys: Vec<String> = fields.unnamed.iter().map(|f| compact_tokens(&f.ty.to_token_stream().to_string())).collect();
                    self.outline.push(format!("  L{:03}: {}struct {}({}){};", line, vis, node.ident, tys.join(", "), doc));
                }
                syn::Fields::Unit => {
                    self.outline.push(format!("  L{:03}: {}struct {};{}", line, vis, node.ident, doc));
                }
            }
        }
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        let ident_str = node.ident.to_string();
        let is_cli = self.is_root_file && (ident_str == "Commands" || ident_str == "Subcommands");

        if is_visible(&node.vis, self.show_all) || is_cli {
            let line = node.span().start().line;
            let doc = extract_first_doc(&node.attrs).map(|d| format!(" // {}", d)).unwrap_or_default();
            let vis = if is_cli && !is_visible(&node.vis, false) { "/* CLI */ ".to_string() } else { format_vis(&node.vis) };
            let variants: Vec<String> = node.variants.iter().map(|v| match &v.fields {
                syn::Fields::Unit => v.ident.to_string(),
                syn::Fields::Unnamed(f) => {
                    let tys: Vec<String> = f.unnamed.iter().map(|f| compact_tokens(&f.ty.to_token_stream().to_string())).collect();
                    format!("{}({})", v.ident, tys.join(", "))
                }
                syn::Fields::Named(_) => format!("{} {{ .. }}", v.ident),
            }).collect();

            self.outline.push(format!(
                "  L{:03}: {}enum {} {{ {} }}{}",
                line, vis, node.ident, variants.join(", "), doc
            ));
        }
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        if is_visible(&node.vis, self.show_all) {
            let line = node.span().start().line;
            let doc = extract_first_doc(&node.attrs).map(|d| format!(" // {}", d)).unwrap_or_default();
            let vis = format_vis(&node.vis);
            self.outline.push(format!("  L{:03}: {}trait {} {{", line, vis, node.ident));
            visit::visit_item_trait(self, node);
            self.outline.push(format!("  }}{}", doc));
        }
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        let line = node.span().start().line;
        let doc = extract_first_doc(&node.attrs).map(|d| format!(" // {}", d)).unwrap_or_default();
        let sig = compact_tokens(&node.sig.to_token_stream().to_string());
        self.outline.push(format!("    L{:03}: {};{}", line, sig, doc));
        visit::visit_trait_item_fn(self, node);
    }

    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
        if is_visible(&node.vis, self.show_all) {
            let line = node.span().start().line;
            let doc = extract_first_doc(&node.attrs).map(|d| format!(" // {}", d)).unwrap_or_default();
            let vis = format_vis(&node.vis);
            self.outline.push(format!("  L{:03}: {}type {} = ...;{}", line, vis, node.ident, doc));
        }
        visit::visit_item_type(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let self_ty = compact_tokens(&node.self_ty.to_token_stream().to_string());
        let old_impl = self.current_impl.clone();

        let impl_header = if let Some((_, trait_path, _)) = &node.trait_ {
            format!("impl {} for {}", compact_tokens(&trait_path.to_token_stream().to_string()), self_ty)
        } else {
            format!("impl {}", self_ty)
        };

        self.current_impl = Some(impl_header.clone());
        let line = node.span().start().line;
        let start_len = self.outline.len();
        self.outline.push(format!("  L{:03}: {} {{", line, impl_header));

        visit::visit_item_impl(self, node);

        if self.outline.len() == start_len + 1 && !self.show_all {
            self.outline.pop();
        } else {
            self.outline.push("  }".to_string());
        }
        self.current_impl = old_impl;
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let is_main = self.is_root_file && node.sig.ident == "main";
        if is_visible(&node.vis, self.show_all) || is_main {
            let line = node.span().start().line;
            let doc = extract_first_doc(&node.attrs).map(|d| format!(" // {}", d)).unwrap_or_default();
            let sig = compact_tokens(&node.sig.to_token_stream().to_string());
            let vis = format_vis(&node.vis);
            self.outline.push(format!("  L{:03}: {}{};{}", line, vis, sig, doc));
        }
        visit::visit_item_fn(self, node);
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let is_vis = is_visible(&node.vis, self.show_all);
        let is_trait_impl = self.current_impl.as_deref().map_or(false, |s| s.contains(" for "));
        if is_vis || is_trait_impl {
            let line = node.span().start().line;
            let doc = extract_first_doc(&node.attrs).map(|d| format!(" // {}", d)).unwrap_or_default();
            let sig = compact_tokens(&node.sig.to_token_stream().to_string());
            let vis = if is_trait_impl { String::new() } else { format_vis(&node.vis) };
            self.outline.push(format!("    L{:03}: {}{};{}", line, vis, sig, doc));
        }
        visit::visit_impl_item_fn(self, node);
    }
}

fn parse_rust_file(path: &Path, show_all: bool) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let syntax = syn::parse_file(&content).ok()?;
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let is_root_file = file_name == "main.rs" || file_name == "lib.rs";

    let mut visitor = RustVisitor {
        show_all,
        is_root_file,
        outline: Vec::new(),
        current_impl: None,
    };
    visitor.visit_file(&syntax);

    if visitor.outline.is_empty() {
        None
    } else {
        Some(visitor.outline.join("\n"))
    }
}

fn extract_readme_pitch(content: &str) -> Option<String> {
    let mut in_frontmatter = false;
    let mut count = 0;

    for line in content.lines().take(40) {
        let trimmed = line.trim();
        if trimmed == "---" {
            count += 1;
            in_frontmatter = count == 1;
            continue;
        }
        if in_frontmatter {
            continue;
        }
        let clean = strip_html_tags(trimmed);
        if clean.len() >= 30
            && !clean.contains("badge")
            && !clean.contains("shield")
            && !clean.contains("github.com")
            && !clean.starts_with("http")
            && !clean.starts_with("[![")
        {
            return Some(clean);
        }
    }
    None
}

fn extract_file_summary(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    if file_name == "main.rs" {
        return Some("CLI Entrypoint & Runtime Loop".to_string());
    }

    if file_name.to_lowercase() == "readme.md" {
        if let Some(pitch) = extract_readme_pitch(&content) {
            return Some(pitch.chars().take(80).collect());
        }
    }

    if ext == "rs" {
        for line in content.lines().take(15) {
            let trimmed = line.trim();
            if trimmed.starts_with("//!") {
                let doc = trimmed.trim_start_matches("//!").trim();
                if !doc.is_empty() {
                    return Some(doc.to_string());
                }
            }
        }
        let mut types = Vec::new();
        let mut funcs = Vec::new();
        for line in content.lines() {
            let t = line.trim();
            if t.starts_with("pub struct ") || t.starts_with("struct ") {
                if let Some(name) = t.split_whitespace().nth(2) {
                    let clean = name.trim_end_matches('{').trim_end_matches(';').trim();
                    if !clean.is_empty() && !types.contains(&clean.to_string()) {
                        types.push(clean.to_string());
                    }
                }
            } else if t.starts_with("pub enum ") || t.starts_with("enum ") {
                if let Some(name) = t.split_whitespace().nth(2) {
                    let clean = name.trim_end_matches('{').trim();
                    if !clean.is_empty() && !types.contains(&clean.to_string()) {
                        types.push(clean.to_string());
                    }
                }
            } else if (t.starts_with("pub fn ") || t.starts_with("pub async fn ")) && funcs.len() < 3 {
                if let Some(fn_part) = t.strip_prefix("pub async fn ").or_else(|| t.strip_prefix("pub fn ")) {
                    if let Some(name) = fn_part.split('(').next() {
                        let clean = name.trim();
                        if !clean.is_empty() && !funcs.contains(&clean.to_string()) {
                            funcs.push(clean.to_string());
                        }
                    }
                }
            }
            if types.len() >= 3 { break; }
        }
        if !types.is_empty() {
            return Some(format!("types: {}", types.join(", ")));
        }
        if !funcs.is_empty() {
            return Some(format!("fn: {}", funcs.join(", ")));
        }
        return None;
    }

    let mut in_frontmatter = false;
    let mut frontmatter_count = 0;

    for line in content.lines().take(30) {
        let trimmed = line.trim();
        if trimmed == "---" {
            frontmatter_count += 1;
            in_frontmatter = frontmatter_count == 1;
            continue;
        }
        if in_frontmatter {
            if trimmed.to_lowercase().starts_with("description:") {
                let desc = trimmed["description:".len()..].trim().trim_matches('"').trim_matches('\'');
                if !desc.is_empty() {
                    return Some(desc.to_string());
                }
            }
            continue;
        }
        if trimmed.starts_with("# ") {
            return Some(trimmed.trim_start_matches("# ").trim().to_string());
        }
        let clean = strip_html_tags(trimmed);
        if clean.len() >= 10
            && !clean.starts_with("use ")
            && !clean.starts_with("http")
            && !clean.starts_with("[![")
        {
            return Some(clean.chars().take(80).collect());
        }
    }
    None
}

fn extract_cargo_meta(root: &Path) -> (Option<String>, Option<String>) {
    let cargo_p = root.join("Cargo.toml");
    let content = match fs::read_to_string(cargo_p) {
        Ok(c) => c,
        Err(_) => return (None, None),
    };

    let mut in_package = false;
    let mut in_deps = false;
    let mut description = None;
    let mut deps = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            in_deps = trimmed == "[dependencies]";
            continue;
        }
        if in_package && trimmed.starts_with("description") {
            if let Some(val) = trimmed.split('=').nth(1) {
                description = Some(val.trim().trim_matches('"').trim_matches('\'').to_string());
            }
        }
        if in_deps && !trimmed.is_empty() && !trimmed.starts_with('#') {
            if let Some(dep) = trimmed.split('=').next() {
                let name = dep.trim();
                if !name.is_empty() {
                    deps.push(name.to_string());
                }
            }
        }
    }

    if description.is_none() {
        if let Ok(readme_c) = fs::read_to_string(root.join("README.md")) {
            description = extract_readme_pitch(&readme_c);
        }
    }

    let deps_str = if deps.is_empty() { None } else { Some(deps.join(", ")) };
    (description, deps_str)
}

fn build_ascii_tree(files: &[(PathBuf, PathBuf)]) -> String {
    let mut lines = Vec::new();
    lines.push("# Active Architecture Tree:".to_string());
    for (i, (full_p, rel_p)) in files.iter().enumerate() {
        let connector = if i == files.len() - 1 { "└── " } else { "├── " };
        let desc = extract_file_summary(full_p).unwrap_or_default();
        let disp = rel_p.display().to_string();
        if !desc.is_empty() {
            lines.push(format!("#   {}{:<32} - {}", connector, disp, desc));
        } else {
            lines.push(format!("#   {}{}", connector, disp));
        }
    }
    lines.join("\n")
}

fn main() {
    let args = Args::parse();
    let target_dir = fs::canonicalize(&args.path).unwrap_or(args.path.clone());
    let root = find_workspace_root(&target_dir);

    let is_subfolder = target_dir != root;
    let target_name = target_dir.file_name().and_then(|n| n.to_str()).unwrap_or("repo");
    let out_filename = if is_subfolder {
        format!("index-map-{}.txt", target_name)
    } else {
        "index-map.txt".to_string()
    };
    let output_file = args.output.unwrap_or_else(|| root.join(out_filename));

    let walker = WalkBuilder::new(&target_dir)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            let is_noise = name.ends_with(".lock")
                || name == "LICENSE"
                || name.ends_with(".svg")
                || name.ends_with(".png")
                || name == "target"
                || name == "node_modules"
                || name == ".git"
                || name == "rs-index"
                || name == ".agent"
                || name.starts_with("index-map");
            !is_noise
        })
        .build();

    let mut all_files = Vec::new();
    let mut rust_files = Vec::new();

    for result in walker {
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };
        let p = entry.path();
        if !p.is_file() {
            continue;
        }

        let rel_p = p.strip_prefix(&target_dir).unwrap_or(p).to_path_buf();
        all_files.push((p.to_path_buf(), rel_p.clone()));

        if p.extension().and_then(|s| s.to_str()) == Some("rs") {
            rust_files.push((p.to_path_buf(), rel_p));
        }
    }

    all_files.sort_by(|a, b| a.1.cmp(&b.1));

    rust_files.sort_by(|a, b| {
        let a_str = a.1.to_string_lossy();
        let b_str = b.1.to_string_lossy();
        let a_is_root = a_str.ends_with("main.rs") || a_str.ends_with("lib.rs");
        let b_is_root = b_str.ends_with("main.rs") || b_str.ends_with("lib.rs");
        match (a_is_root, b_is_root) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.1.cmp(&b.1),
        }
    });

    let (desc_opt, deps_opt) = extract_cargo_meta(&root);

    let mut out = String::new();
    out.push_str(&format!(
        "# CODEBASE ARCHITECTURE MAP\n# Root: {}\n# Target: {}\n",
        root.display(),
        target_dir.display()
    ));

    if let Some(desc) = desc_opt {
        out.push_str(&format!("# Purpose: {}\n", desc));
    }
    if let Some(deps) = deps_opt {
        out.push_str(&format!("# Dependencies: {}\n", deps));
    }
    out.push_str("# --------------------------------------------------\n\n");

    out.push_str(&build_ascii_tree(&all_files));
    out.push_str("\n# --------------------------------------------------\n\n## Rust API Skeleton:\n");

    for (full_path, rel_path) in rust_files {
        if let Some(skeleton) = parse_rust_file(&full_path, args.all) {
            out.push_str(&format!("\n### `{}`\n```rust\n{}\n```\n", rel_path.display(), skeleton));
        }
    }

    if args.print {
        print!("{}", out);
    } else {
        fs::write(&output_file, &out).expect("Failed to write map file");
        eprintln!("\x1b[1;32m[ok]\x1b[0m Map generated: {}", output_file.display());
    }
}
