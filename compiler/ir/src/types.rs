use nex_ast::BinOp as AstBinOp;
use nex_semantic::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IrType {
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
    /// User-defined record type, emitted as `%struct.<name>` in LLVM IR.
    Struct(&'static str),
    /// Fixed-size array, spelled as its LLVM type (`[4 x i64]`).
    Array(&'static str),
    Void,
}

impl IrType {
    pub fn from_type(ty: Type) -> Self {
        match ty {
            Type::Bool => Self::Bool,
            Type::Char => Self::Char,
            Type::I8 => Self::I8,
            Type::I16 => Self::I16,
            Type::I32 => Self::I32,
            Type::I64 => Self::I64,
            Type::U8 => Self::U8,
            Type::U16 => Self::U16,
            Type::U32 => Self::U32,
            Type::U64 => Self::U64,
            Type::F32 => Self::F32,
            Type::F64 => Self::F64,
            Type::String => Self::String,
            Type::Struct(name) => Self::Struct(name),
            Type::Array { llvm, .. } => Self::Array(llvm),
            // Growable arrays are handled through a header pointer: the value is a\n            // plain pointer to `{ ptr, i64, i64 }`, so variables store one word.
            Type::DynArray(_) => Self::String,
            Type::Enum(_) => Self::I32,
            Type::TypeParam(_) => Self::Void, // should never reach IR
            Type::Void => Self::Void,
            Type::Error => Self::Void,
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

    pub const fn is_signed(self) -> bool {
        matches!(self, Self::I8 | Self::I16 | Self::I32 | Self::I64)
    }

    pub const fn is_unsigned(self) -> bool {
        matches!(self, Self::U8 | Self::U16 | Self::U32 | Self::U64)
    }

    pub const fn is_float(self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }

    pub const fn is_pointer(self) -> bool {
        matches!(self, Self::String)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

impl BinOp {
    pub fn from_ast(op: AstBinOp) -> Option<Self> {
        Some(match op {
            AstBinOp::Add => Self::Add,
            AstBinOp::Sub => Self::Sub,
            AstBinOp::Mul => Self::Mul,
            AstBinOp::Div => Self::Div,
            AstBinOp::Rem => Self::Rem,
            AstBinOp::Eq => Self::Eq,
            AstBinOp::Ne => Self::Ne,
            AstBinOp::Lt => Self::Lt,
            AstBinOp::Le => Self::Le,
            AstBinOp::Gt => Self::Gt,
            AstBinOp::Ge => Self::Ge,
            AstBinOp::BitAnd => Self::BitAnd,
            AstBinOp::BitOr => Self::BitOr,
            AstBinOp::BitXor => Self::BitXor,
            AstBinOp::Shl => Self::Shl,
            AstBinOp::Shr => Self::Shr,
            AstBinOp::And | AstBinOp::Or => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnOp {
    Neg,
    Not,
    BitNot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CastKind {
    SignExtend,
    ZeroExtend,
    FloatExtend,
    Truncate,
    SignedIntToFloat,
    UnsignedIntToFloat,
    FloatToSignedInt,
    FloatToUnsignedInt,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Const {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Char(char),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    Value(ValueId),
    Const(Const),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValueId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub u32);

#[derive(Debug, Clone)]
pub enum Inst {
    Alloca {
        dst: ValueId,
        ty: IrType,
    },
    Store {
        ptr: ValueId,
        value: Operand,
        ty: IrType,
    },
    Load {
        dst: ValueId,
        ptr: ValueId,
        ty: IrType,
    },
    Bin {
        dst: ValueId,
        op: BinOp,
        ty: IrType,
        lhs: Operand,
        rhs: Operand,
    },
    Un {
        dst: ValueId,
        op: UnOp,
        ty: IrType,
        operand: Operand,
    },
    Cast {
        dst: ValueId,
        kind: CastKind,
        from: IrType,
        to: IrType,
        operand: Operand,
    },
    Call {
        dst: Option<ValueId>,
        name: String,
        args: Vec<Operand>,
        ret: IrType,
    },
    /// Compute a pointer to a struct field: `getelementptr inbounds %struct.X,
    /// ptr %base, i32 0, i32 <index>`.
    FieldPtr {
        dst: ValueId,
        base: ValueId,
        struct_name: &'static str,
        index: u32,
        field_ty: IrType,
    },
    /// Compute a pointer to an array element: `getelementptr inbounds [N x T],
    /// ptr %base, i64 0, i64 %index`.
    IndexPtr {
        dst: ValueId,
        base: ValueId,
        array_type: &'static str,
        index: Operand,
        elem_ty: IrType,
    },

    /// Address of a function symbol as an integer:
    /// `ptrtoint ptr @name to i64`. Used by `thread_spawn`.
    FuncPtr {
        dst: ValueId,
        name: String,
    },
    /// Field address inside the growable array header:
    /// `getelementptr inbounds { ptr, i64, i64 }, ptr %header, i32 0, i32 N`.
    HeaderPtr {
        dst: ValueId,
        base: ValueId,
        index: u32,
    },
    /// Compute a pointer into a growable array's data buffer:
    /// `getelementptr inbounds <elem>, ptr %data, i64 %index`. Bounds checking is
    /// emitted separately against the length stored in the array header.
    DynIndexPtr {
        dst: ValueId,
        base: ValueId,
        index: Operand,
        elem_ty: IrType,
    },
}

#[derive(Debug, Clone)]
pub enum Terminator {
    Ret(Option<Operand>),
    Br(BlockId),
    CondBr {
        cond: Operand,
        then_bb: BlockId,
        else_bb: BlockId,
    },
    Unreachable,
}

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: BlockId,
    pub insts: Vec<Inst>,
    pub terminator: Option<Terminator>,
}

#[derive(Debug, Clone)]
pub struct IrFunction {
    pub name: String,
    pub params: Vec<(ValueId, IrType)>,
    pub ret: IrType,
    pub blocks: Vec<BasicBlock>,
}

#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: &'static str,
    pub fields: Vec<IrType>,
}

#[derive(Debug, Clone)]
pub struct ExternDecl {
    pub name: String,
    pub params: Vec<IrType>,
    pub ret: IrType,
}

#[derive(Debug, Clone)]
pub struct IrModule {
    pub structs: Vec<StructDef>,
    pub functions: Vec<IrFunction>,
    pub externs: Vec<ExternDecl>,
}
