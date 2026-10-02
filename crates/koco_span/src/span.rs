use std::fmt;
use std::ops::Range;

/// Index of a file inside a [`crate::SourceMap`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(u32);

impl From<usize> for FileId {
    fn from(id: usize) -> FileId { FileId(u32::try_from(id).expect("too many source files")) }
}

impl From<FileId> for usize {
    fn from(id: FileId) -> usize { id.0 as usize }
}

// Prints transparently so `Span`'s debug output is unchanged by this being a newtype.
impl fmt::Debug for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Span {
    pub file: FileId,
    pub lo: u32,
    pub hi: u32,
}

impl Span {
    pub fn new(file: FileId, lo: usize, hi: usize) -> Span {
        Span {
            file,
            lo: lo as u32,
            hi: hi as u32,
        }
    }

    pub fn to(self, other: Span) -> Span {
        debug_assert_eq!(self.file, other.file, "cannot join spans from different files");
        Span {
            file: self.file,
            lo: self.lo.min(other.lo),
            hi: self.hi.max(other.hi),
        }
    }

    pub fn shrink_to_hi(self) -> Span {
        Span {
            file: self.file,
            lo: self.hi,
            hi: self.hi,
        }
    }

    pub fn range(self) -> Range<usize> { self.lo as usize..self.hi as usize }

    pub fn len(self) -> usize { (self.hi - self.lo) as usize }

    pub fn is_empty(self) -> bool { self.hi == self.lo }
}
