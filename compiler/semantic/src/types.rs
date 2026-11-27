use std::fmt;
use std::sync::Mutex;

/// Types implemented by the NEX compiler.
///
/// Planned types such as `bytes`, `array<T>`, `optional<T>` and user-defined
/// structs are intentionally absent until the corresponding runtime, layout and
/// ABI rules are implemented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Type {
    Bool,
    Char,
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    String,
    /// User-defined record type. The name is interned by semantic analysis and
    /// lives for the duration of the compiler process.
    Struct(&'static str),
    /// User-defined enumeration, lowered to an `i32` tag. The name is interned
    /// by semantic analysis.
    Enum(&'static str),
    /// Fixed-size array. `display` is the diagnostic spelling (`[i64; 4]`);
    /// `llvm` is the LLVM type spelling (`[4 x i64]`).
    Array {
        display: &'static str,
        llvm: &'static str,
    },
    /// Growable array: the interned display name (for example `[i64]`) is enough
    /// to find the element type in `Analysis::arrays`. See
    /// `specs/dynamic-arrays.md`.
    DynArray(&'static str),
    /// Shared (`&T`) or mutable (`&mut T`) reference to a value of the interned
    /// pointee type name. References are represented as a plain pointer at the
    /// IR level; the pointee is resolved on demand from its name.
    Ref {
        inner: &'static str,
        mutable: bool,
    },
    /// Generic type parameter. Used only during semantic analysis of generic
    /// declarations; concrete instantiations replace it with a real type.
    TypeParam(&'static str),
    Void,
    Error,
}

impl Type {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "bool" => Self::Bool,
            "char" => Self::Char,
            "i8" => Self::I8,
            "i16" => Self::I16,
            "i32" => Self::I32,
            "i64" => Self::I64,
            "u8" => Self::U8,
            "u16" => Self::U16,
            "u32" => Self::U32,
            "u64" => Self::U64,
            "f32" => Self::F32,
            "f64" => Self::F64,
            "string" => Self::String,
            "void" => Self::Void,
            _ => return None,
        })
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Char => "char",
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::String => "string",
            Self::Struct(name) => name,
            Self::Enum(name) => name,
            Self::Array { display, .. } => display,
            Self::DynArray(display) => display,
            Self::Ref { mutable, .. } => {
                if *mutable {
                    "&mut T"
                } else {
                    "&T"
                }
            }
            Self::TypeParam(name) => name,
            Self::Void => "void",
            Self::Error => "<error>",
        }
    }

    pub const fn is_integer(self) -> bool {
        matches!(
            self,
            Self::I8
                | Self::I16
                | Self::I32
                | Self::I64
                | Self::U8
                | Self::U16
                | Self::U32
                | Self::U64
        )
    }

    pub const fn is_signed_integer(self) -> bool {
        matches!(self, Self::I8 | Self::I16 | Self::I32 | Self::I64)
    }

    pub const fn is_unsigned_integer(self) -> bool {
        matches!(self, Self::U8 | Self::U16 | Self::U32 | Self::U64)
    }

    pub const fn is_float(self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }

    pub const fn is_numeric(self) -> bool {
        self.is_integer() || self.is_float()
    }

    /// Types that own heap memory and therefore have move (not copy) semantics.
    ///
    /// Scalars, fixed arrays, tuples, and structs keep value (copy) semantics.
    /// `string` and growable arrays `[T]` own a heap allocation, so binding them
    /// to another variable moves ownership and the source becomes unusable.
    pub const fn is_move_only(self) -> bool {
        matches!(self, Self::String | Self::DynArray(_))
    }

    pub const fn is_printable(self) -> bool {
        matches!(
            self,
            Self::Bool
                | Self::Char
                | Self::I8
                | Self::I16
                | Self::I32
                | Self::I64
                | Self::U8
                | Self::U16
                | Self::U32
                | Self::U64
                | Self::F32
                | Self::F64
                | Self::String
                | Self::Struct(_)
                | Self::Array { .. }
        )
    }

    pub fn integer_range(self) -> Option<(i128, i128)> {
        Some(match self {
            Self::I8 => (i8::MIN as i128, i8::MAX as i128),
            Self::I16 => (i16::MIN as i128, i16::MAX as i128),
            Self::I32 => (i32::MIN as i128, i32::MAX as i128),
            Self::I64 => (i64::MIN as i128, i64::MAX as i128),
            Self::U8 => (0, u8::MAX as i128),
            Self::U16 => (0, u16::MAX as i128),
            Self::U32 => (0, u32::MAX as i128),
            Self::U64 => (0, u64::MAX as i128),
            _ => return None,
        })
    }
}

impl fmt::Display for Type {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// Process-wide intern cache for `&'static str` type names.
///
/// The compiler interns type names as `&'static str` (see `Type`). Historically
/// every intern site called `Box::leak`, so a long-running process such as
/// `nex lsp` leaked one allocation per distinct type name per analysis. Routing
/// all intern sites through this cache makes repeated names reuse the same
/// allocation; only genuinely new names leak, and their count is bounded by the
/// number of distinct types in the program.
static INTERN_CACHE: Mutex<Option<std::collections::HashMap<String, &'static str>>> =
    Mutex::new(None);

pub fn intern(s: String) -> &'static str {
    let mut guard = INTERN_CACHE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let cache = guard.get_or_insert_with(std::collections::HashMap::new);
    if let Some(existing) = cache.get(&s) {
        return existing;
    }
    let leaked: &'static str = Box::leak(s.clone().into_boxed_str());
    cache.insert(s, leaked);
    leaked
}
