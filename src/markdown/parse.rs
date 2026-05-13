use anyhow::Result;
use tree_sitter::Parser;

pub fn parse_markdown(source: &str) -> Result<()> {
    let mut parser = Parser::new();
    parser.set_language(tree_sitter_markdown::language())?;
    let _tree = parser.parse(source, None);
    Ok(())
}
