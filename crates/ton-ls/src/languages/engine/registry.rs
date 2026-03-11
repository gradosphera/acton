use crate::backend::utils::SourceLanguage;
use crate::languages::engine::adapter::{FiftSyntaxAdapter, TasmSyntaxAdapter, TomlSyntaxAdapter};
use crate::languages::engine::cache::{IncrementalParseCache, ParsedSnapshot};
use lsp_types::{TextDocumentContentChangeEvent, Url};

#[derive(Default)]
pub struct SelfContainedLanguageRegistry {
    tasm_cache: IncrementalParseCache<TasmSyntaxAdapter>,
    fift_cache: IncrementalParseCache<FiftSyntaxAdapter>,
    toml_cache: IncrementalParseCache<TomlSyntaxAdapter>,
}

impl SelfContainedLanguageRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn did_open(
        &self,
        language: SourceLanguage,
        uri: &Url,
        version: i32,
        text: &str,
    ) -> anyhow::Result<()> {
        match language {
            SourceLanguage::Tasm => {
                self.tasm_cache.open(uri, version, text)?;
            }
            SourceLanguage::Fift => {
                self.fift_cache.open(uri, version, text)?;
            }
            SourceLanguage::Toml => {
                self.toml_cache.open(uri, version, text)?;
            }
            SourceLanguage::Tolk | SourceLanguage::Unknown => {}
        }
        Ok(())
    }

    pub fn did_change(
        &self,
        language: SourceLanguage,
        uri: &Url,
        version: i32,
        changes: &[TextDocumentContentChangeEvent],
    ) -> anyhow::Result<Option<String>> {
        match language {
            SourceLanguage::Tasm => Ok(self
                .tasm_cache
                .sync_changes(uri, version, changes)?
                .map(|snapshot| snapshot.text.to_string())),
            SourceLanguage::Fift => Ok(self
                .fift_cache
                .sync_changes(uri, version, changes)?
                .map(|snapshot| snapshot.text.to_string())),
            SourceLanguage::Toml => Ok(self
                .toml_cache
                .sync_changes(uri, version, changes)?
                .map(|snapshot| snapshot.text.to_string())),
            SourceLanguage::Tolk | SourceLanguage::Unknown => Ok(None),
        }
    }

    pub fn did_close(&self, language: SourceLanguage, uri: &Url) {
        match language {
            SourceLanguage::Tasm => {
                self.tasm_cache.remove(uri);
            }
            SourceLanguage::Fift => {
                self.fift_cache.remove(uri);
            }
            SourceLanguage::Toml => {
                self.toml_cache.remove(uri);
            }
            SourceLanguage::Tolk | SourceLanguage::Unknown => {}
        }
    }

    pub fn find_tasm_file(&self, uri: &Url) -> Option<ParsedSnapshot<tasm_syntax::SourceFile>> {
        self.tasm_cache.snapshot(uri)
    }

    pub fn find_fift_file(&self, uri: &Url) -> Option<ParsedSnapshot<fift_syntax::SourceFile>> {
        self.fift_cache.snapshot(uri)
    }

    pub fn find_toml_file(&self, uri: &Url) -> Option<ParsedSnapshot<toml_syntax::SourceFile>> {
        self.toml_cache.snapshot(uri)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{Position, Range};

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
            text: text.to_owned(),
        }
    }

    #[test]
    fn tracks_tasm_lifecycle() -> anyhow::Result<()> {
        let registry = SelfContainedLanguageRegistry::new();
        let uri = Url::parse("file:///tmp/registry.tasm")?;

        registry.did_open(SourceLanguage::Tasm, &uri, 1, "PUSHINT_4 1\n")?;
        assert!(registry.find_tasm_file(&uri).is_some());

        let changes = vec![change(Some(range(0, 10, 0, 11)), "2")];
        registry.did_change(SourceLanguage::Tasm, &uri, 2, &changes)?;

        let snapshot = registry
            .find_tasm_file(&uri)
            .expect("tasm snapshot should exist");
        assert_eq!(snapshot.text.as_ref(), "PUSHINT_4 2\n");
        assert_eq!(snapshot.version, 2);

        registry.did_close(SourceLanguage::Tasm, &uri);
        assert!(registry.find_tasm_file(&uri).is_none());
        Ok(())
    }

    #[test]
    fn does_not_create_snapshot_on_change_without_open() -> anyhow::Result<()> {
        let registry = SelfContainedLanguageRegistry::new();
        let uri = Url::parse("file:///tmp/registry-missing.tasm")?;
        let changes = vec![change(Some(range(0, 0, 0, 0)), "PUSHINT_4 1\n")];

        let updated_text = registry.did_change(SourceLanguage::Tasm, &uri, 1, &changes)?;
        assert!(updated_text.is_none());
        assert!(registry.find_tasm_file(&uri).is_none());
        Ok(())
    }
}
