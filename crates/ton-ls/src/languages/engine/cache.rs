use crate::languages::engine::adapter::SyntaxAdapter;
use crate::languages::engine::edits::apply_lsp_changes;
use dashmap::DashMap;
use lsp_types::{TextDocumentContentChangeEvent, Url};
use std::marker::PhantomData;
use std::sync::Arc;
use tree_sitter::InputEdit;

#[derive(Debug, Clone)]
pub struct ParsedSnapshot<TSourceFile> {
    pub uri: Url,
    pub version: i32,
    pub text: Arc<str>,
    pub source_file: Arc<TSourceFile>,
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
        let snapshot = ParsedSnapshot {
            uri: uri.clone(),
            version,
            text: Arc::from(text),
            source_file: Arc::new(parsed),
        };

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

        let snapshot = ParsedSnapshot {
            uri: uri.clone(),
            version,
            text: Arc::from(applied.text),
            source_file: Arc::new(parsed),
        };

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

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::bail;
    use lsp_types::{Position, Range, TextDocumentContentChangeEvent};
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
}
