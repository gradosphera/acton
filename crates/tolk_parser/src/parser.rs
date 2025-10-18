use tree_sitter::Parser;

pub fn parse(code: impl AsRef<[u8]>) -> Result<tree_sitter::Tree, anyhow::Error> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_tolk::language())?;

    let source_code = code;
    let tree = parser.parse(source_code, None).unwrap();
    Ok(tree)
}
