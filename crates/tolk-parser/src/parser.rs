use tree_sitter::{Language, Parser};

pub fn parse(code: impl AsRef<[u8]>) -> anyhow::Result<tree_sitter::Tree> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_tolk::LANGUAGE.into())?;

    let source_code = code;
    let Some(tree) = parser.parse(source_code, None) else {
        anyhow::bail!("cannot parse Tolk file");
    };
    Ok(tree)
}

pub fn language() -> Language {
    tree_sitter_tolk::LANGUAGE.into()
}
