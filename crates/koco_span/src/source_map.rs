use crate::{FileId, Span};
use codespan_reporting::files::SimpleFiles;

pub struct SourceMap {
    files: SimpleFiles<String, String>,
}

impl SourceMap {
    pub fn new() -> SourceMap {
        SourceMap {
            files: SimpleFiles::new(),
        }
    }

    pub fn add_file(&mut self, name: impl Into<String>, src: impl Into<String>) -> FileId {
        let src = src.into();
        assert!(u32::try_from(src.len()).is_ok(), "source file too large");
        self.files.add(name.into(), src).into()
    }

    pub fn name(&self, id: FileId) -> &str { self.files.get(id.into()).expect("invalid FileId").name() }

    pub fn source(&self, id: FileId) -> &str { self.files.get(id.into()).expect("invalid FileId").source() }

    pub fn snippet(&self, span: Span) -> &str { &self.source(span.file)[span.range()] }

    pub fn codespan_files(&self) -> &SimpleFiles<String, String> { &self.files }
}
