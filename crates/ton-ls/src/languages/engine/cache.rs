use crate::languages::engine::adapter::SyntaxAdapter;
use crate::languages::engine::edits::apply_lsp_changes;
use crate::languages::fift::traverse::PreorderTraverse;
use dashmap::DashMap;
use lsp_types::{Position, Range, TextDocumentContentChangeEvent, Url};
use std::marker::PhantomData;
use std::sync::Arc;
use tree_sitter::{InputEdit, Node, Point};

pub trait HasRootNode {
    fn root_node<'tree>(&'tree self) -> Node<'tree>;
}

impl HasRootNode for tasm_syntax::SourceFile {
    fn root_node<'tree>(&'tree self) -> Node<'tree> {
        self.tree.root_node()
    }
}

impl HasRootNode for fift_syntax::SourceFile {
    fn root_node<'tree>(&'tree self) -> Node<'tree> {
        self.tree.root_node()
    }
}

impl HasRootNode for toml_syntax::SourceFile {
    fn root_node<'tree>(&'tree self) -> Node<'tree> {
        self.tree.root_node()
    }
}

#[derive(Debug, Clone)]
pub struct ParsedSnapshot<TSourceFile> {
    pub uri: Url,
    pub version: i32,
    pub text: Arc<str>,
    pub source_file: Arc<TSourceFile>,
    pub(crate) line_offsets: Arc<Vec<usize>>,
}

impl<TSourceFile> ParsedSnapshot<TSourceFile> {
    #[must_use]
    pub fn source(&self) -> &str {
        self.text.as_ref()
    }

    #[must_use]
    pub fn syntax(&self) -> &TSourceFile {
        self.source_file.as_ref()
    }

    pub fn line_offsets(&self) -> &[usize] {
        &self.line_offsets
    }

    pub fn point(&self, position: Position) -> Point {
        position_to_point(&self.line_offsets, self.source(), position)
    }

    pub fn position_to_offset(&self, position: Position) -> usize {
        point_to_offset(self.source(), &self.line_offsets, self.point(position))
    }

    pub fn position(&self, offset: usize) -> Position {
        offset_to_position(self.line_offsets.as_ref(), self.source(), offset)
    }

    pub fn range_of(&self, node: Node) -> Range {
        self.position_range(node.start_byte(), node.end_byte())
    }

    pub(crate) fn position_range(&self, start_offset: usize, end_offset: usize) -> Range {
        Range::new(self.position(start_offset), self.position(end_offset))
    }

    pub fn text_of<'tree>(&'tree self, node: Node<'tree>) -> &'tree str {
        node.utf8_text(self.text.as_bytes()).unwrap_or("<invalid>")
    }

    pub fn new(
        uri: Url,
        version: i32,
        text: impl Into<Arc<str>>,
        source_file: Arc<TSourceFile>,
    ) -> Self {
        let text = text.into();
        let line_offsets = Arc::new(build_line_offsets(&text));

        Self {
            uri,
            version,
            text,
            source_file,
            line_offsets,
        }
    }
}

impl<TSourceFile: HasRootNode> ParsedSnapshot<TSourceFile> {
    pub fn traverse(&self) -> PreorderTraverse<'_> {
        PreorderTraverse::new(self.syntax().root_node().walk())
    }

    pub fn node_at(&self, position: Position) -> Option<Node<'_>> {
        let point = self.point(position);
        self.syntax()
            .root_node()
            .descendant_for_point_range(point, point)
    }

    pub fn find_node_at(&self, position: Position) -> Option<Node<'_>> {
        self.node_at(position)
    }
}

pub struct IncrementalParseCache<A: SyntaxAdapter> {
    docs: DashMap<Url, ParsedSnapshot<A::SourceFile>>,
    _adapter: PhantomData<A>,
}

impl<A: SyntaxAdapter> IncrementalParseCache<A> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            docs: DashMap::new(),
            _adapter: PhantomData,
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.docs.len()
    }

    pub fn remove(&self, uri: &Url) -> Option<ParsedSnapshot<A::SourceFile>> {
        self.docs.remove(uri).map(|(_, snapshot)| snapshot)
    }

    pub fn snapshot(&self, uri: &Url) -> Option<ParsedSnapshot<A::SourceFile>> {
        self.docs.get(uri).map(|snapshot| snapshot.clone())
    }

    pub fn open(
        &self,
        uri: &Url,
        version: i32,
        text: &str,
    ) -> anyhow::Result<ParsedSnapshot<A::SourceFile>> {
        let parsed = A::parse(text)?;
        let snapshot = ParsedSnapshot::new(uri.clone(), version, Arc::from(text), Arc::new(parsed));

        self.docs.insert(uri.clone(), snapshot.clone());
        Ok(snapshot)
    }

    pub fn sync_changes(
        &self,
        uri: &Url,
        version: i32,
        changes: &[TextDocumentContentChangeEvent],
    ) -> anyhow::Result<Option<ParsedSnapshot<A::SourceFile>>> {
        let Some(current) = self.snapshot(uri) else {
            return Ok(None);
        };

        if version < current.version {
            return Ok(Some(current));
        }

        let applied = apply_lsp_changes(current.text.as_ref(), changes);
        let parsed = parse_with_incremental_fallback::<A>(
            &current,
            &applied.text,
            applied.incremental_edits.as_deref(),
        )?;

        let snapshot = ParsedSnapshot::new(
            uri.clone(),
            version,
            Arc::from(applied.text),
            Arc::new(parsed),
        );

        self.docs.insert(uri.clone(), snapshot.clone());
        Ok(Some(snapshot))
    }
}

impl<A: SyntaxAdapter> Default for IncrementalParseCache<A> {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_with_incremental_fallback<A: SyntaxAdapter>(
    current: &ParsedSnapshot<A::SourceFile>,
    source: &str,
    incremental_edits: Option<&[InputEdit]>,
) -> anyhow::Result<A::SourceFile> {
    let Some(incremental_edits) = incremental_edits else {
        return A::parse(source);
    };

    let mut old_tree = A::tree(current.source_file.as_ref()).clone();
    for edit in incremental_edits {
        old_tree.edit(edit);
    }

    match A::parse_with_old_tree(source, Some(&old_tree)) {
        Ok(parsed) => Ok(parsed),
        Err(incremental_error) => {
            log::debug!(
                "incremental parse failed, falling back to full parse: {incremental_error}"
            );
            A::parse(source)
        }
    }
}

fn build_line_offsets(text: &str) -> Vec<usize> {
    let mut offsets = Vec::new();
    offsets.push(0);

    let mut last_offset = 0;
    for line in text.lines() {
        last_offset += line.len() + 1;
        offsets.push(last_offset);
    }

    offsets
}

fn position_to_point(line_offsets: &[usize], source: &str, position: Position) -> Point {
    let line_start = line_offsets
        .get(position.line as usize)
        .copied()
        .unwrap_or_else(|| *line_offsets.last().unwrap_or(&0));
    if line_start >= source.len() {
        return Point::new(position.line as usize, 0);
    }

    let mut byte_column = 0;
    let mut utf16_column = 0;
    for ch in source[line_start..].chars() {
        if utf16_column >= position.character as usize {
            break;
        }
        byte_column += ch.len_utf8();
        utf16_column += ch.len_utf16();
    }

    Point::new(position.line as usize, byte_column)
}

fn point_to_offset(source: &str, line_offsets: &[usize], point: Point) -> usize {
    let line_start = line_offsets
        .get(point.row)
        .copied()
        .unwrap_or_else(|| *line_offsets.last().unwrap_or(&0));

    if line_start >= source.len() {
        return line_start;
    }

    let mut byte_column = 0;
    for ch in source[line_start..].chars() {
        if byte_column >= point.column {
            break;
        }
        byte_column += ch.len_utf8();
    }

    line_start + byte_column
}

fn offset_to_position(line_offsets: &[usize], source: &str, offset: usize) -> Position {
    if source.is_empty() {
        return Position::new(0, 0);
    }

    let clamped_offset = offset.min(source.len());
    let line = line_offsets
        .binary_search(&clamped_offset)
        .unwrap_or_else(|idx| idx.saturating_sub(1));

    if line >= line_offsets.len() {
        return Position::new(line_offsets.len() as u32, 0);
    }

    let line_start = line_offsets.get(line).copied().unwrap_or_default();
    if line_start >= source.len() {
        return Position::new(line as u32, 0);
    }

    let col_byte_offset = offset.saturating_sub(line_start);
    let mut byte_count = 0usize;
    let mut utf16_count = 0usize;
    for ch in source[line_start..].chars() {
        if byte_count >= col_byte_offset {
            break;
        }
        byte_count += ch.len_utf8();
        utf16_count += ch.len_utf16();
    }

    Position::new(line as u32, utf16_count as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::bail;
    use lsp_types::{Position, Range, TextDocumentContentChangeEvent};
    use std::sync::Arc;
    use tree_sitter::Tree;

    use crate::languages::engine::adapter::{SyntaxAdapter, TasmSyntaxAdapter};

    fn pos(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    fn range(start_line: u32, start_character: u32, end_line: u32, end_character: u32) -> Range {
        Range {
            start: pos(start_line, start_character),
            end: pos(end_line, end_character),
        }
    }

    fn change(range: Option<Range>, text: &str) -> TextDocumentContentChangeEvent {
        TextDocumentContentChangeEvent {
            range,
            range_length: None,
            text: text.to_string(),
        }
    }

    #[test]
    fn applies_incremental_reparse() -> anyhow::Result<()> {
        let cache = IncrementalParseCache::<TasmSyntaxAdapter>::new();
        let uri = Url::parse("file:///tmp/cache_test.tasm")?;

        cache.open(&uri, 1, "PUSHINT_4 1\n")?;
        let changes = vec![change(Some(range(0, 10, 0, 11)), "2")];

        let updated = cache
            .sync_changes(&uri, 2, &changes)?
            .expect("snapshot should be present");

        assert_eq!(updated.version, 2);
        assert_eq!(updated.text.as_ref(), "PUSHINT_4 2\n");
        assert_eq!(updated.source_file.top_levels().count(), 1);
        Ok(())
    }

    #[derive(Debug, Clone, Copy, Default)]
    struct ForcedFallbackAdapter;

    impl SyntaxAdapter for ForcedFallbackAdapter {
        type SourceFile = tasm_syntax::SourceFile;

        fn parse(source: &str) -> anyhow::Result<Self::SourceFile> {
            tasm_syntax::parse(source)
        }

        fn parse_with_old_tree(
            _source: &str,
            old_tree: Option<&Tree>,
        ) -> anyhow::Result<Self::SourceFile> {
            if old_tree.is_some() {
                bail!("forced incremental parse error");
            }
            unreachable!("incremental parse path should always pass old_tree")
        }

        fn tree(source_file: &Self::SourceFile) -> &Tree {
            &source_file.tree
        }
    }

    #[test]
    fn falls_back_to_full_parse_when_incremental_fails() -> anyhow::Result<()> {
        let cache = IncrementalParseCache::<ForcedFallbackAdapter>::new();
        let uri = Url::parse("file:///tmp/cache_fallback.tasm")?;

        cache.open(&uri, 1, "PUSHINT_4 1\n")?;
        let changes = vec![change(Some(range(0, 10, 0, 11)), "2")];

        let updated = cache
            .sync_changes(&uri, 2, &changes)?
            .expect("snapshot should be present");

        assert_eq!(updated.text.as_ref(), "PUSHINT_4 2\n");
        assert_eq!(updated.source_file.top_levels().count(), 1);
        Ok(())
    }

    #[test]
    fn snapshot_position_and_offset_roundtrip() {
        let uri = Url::parse("file:///tmp/position_snapshot.tasm").expect("uri should parse");
        let text = "a😀b\n😄";
        let source_file = Arc::new(tasm_syntax::parse(text).expect("sample text should parse"));
        let snapshot = ParsedSnapshot::new(uri, 1, text, source_file);

        let position = Position::new(0, 3);
        let point = snapshot.point(position);
        assert_eq!(point.row, 0);
        assert_eq!(point.column, 5);

        let byte_offset = snapshot.position_to_offset(position);
        assert_eq!(byte_offset, 5);
        assert_eq!(snapshot.position(byte_offset), position);
    }
}
