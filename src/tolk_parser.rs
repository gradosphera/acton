use tree_sitter::Parser;

pub fn parse(code: impl AsRef<[u8]>) -> tree_sitter::Tree {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_tolk::language())
        .expect("Error loading Rust grammar");

    let source_code = code;
    let tree = parser.parse(source_code, None).unwrap();
    tree
}
