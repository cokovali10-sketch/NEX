use std::collections::HashMap;

use nex_ir::{
    BinOp, BlockId, CastKind, Const, Inst, IrFunction, IrModule, IrType, Operand, Terminator, UnOp,
};

const RUNTIME_DECLS: &[&str] = &[
    "declare void @nex_rt_print_str(ptr)",
    "declare void @nex_rt_println_str(ptr)",
    "declare void @nex_rt_print_i64(i64)",
    "declare void @nex_rt_println_i64(i64)",
    "declare void @nex_rt_print_u64(i64)",
    "declare void @nex_rt_println_u64(i64)",
    "declare void @nex_rt_print_f64(double)",
    "declare void @nex_rt_println_f64(double)",
    "declare void @nex_rt_print_bool(i32)",
    "declare void @nex_rt_println_bool(i32)",
    "declare void @nex_rt_print_char(i32)",
    "declare void @nex_rt_println_char(i32)",
    "declare i32 @nex_rt_str_eq(ptr, ptr)",
    "declare i32 @nex_rt_str_cmp(ptr, ptr)",
    "declare i64 @nex_rt_len(ptr)",
    "declare i64 @nex_rt_abs(i64)",
    "declare i64 @nex_rt_alloc(i64)",
    "declare void @nex_rt_free(i64)",
    "declare i64 @nex_rt_peek_i64(i64)",
    "declare void @nex_rt_poke_i64(i64, i64)",
    "declare ptr @nex_rt_str_concat(ptr, ptr)",
    "declare i32 @nex_rt_str_contains(ptr, ptr)",
    "declare i64 @nex_rt_str_index_of(ptr, ptr)",
    "declare ptr @nex_rt_str_substring(ptr, i64, i64)",
    "declare ptr @nex_rt_str_upper(ptr)",
    "declare ptr @nex_rt_str_lower(ptr)",
    "declare ptr @nex_rt_str_trim(ptr)",
    "declare i32 @nex_rt_str_starts_with(ptr, ptr)",
    "declare i32 @nex_rt_str_ends_with(ptr, ptr)",
    "declare ptr @nex_rt_str_repeat(ptr, i64)",
    "declare ptr @nex_rt_str_replace(ptr, ptr, ptr)",
    "declare ptr @nex_rt_str_reverse(ptr)",
    "declare i64 @nex_rt_parse_i64(ptr)",
    "declare double @nex_rt_abs_f64(double)",
    "declare ptr @nex_rt_f64_to_string(double)",
    "declare ptr @nex_rt_bool_to_string(i32)",
    "declare ptr @nex_rt_read_file(ptr)",
    "declare i32 @nex_rt_write_file(ptr, ptr)",
    "declare i32 @nex_rt_file_exists(ptr)",
    "declare i64 @nex_rt_file_size(ptr)",
    "declare i64 @nex_rt_tcp_connect(ptr, i64)",
    "declare i64 @nex_rt_tcp_listen(i64, i64)",
    "declare i64 @nex_rt_tcp_accept(i64)",
    "declare i64 @nex_rt_tcp_write(i64, ptr, i64)",
    "declare ptr @nex_rt_tcp_read(i64, i64)",
    "declare i32 @nex_rt_tcp_close(i64)",
    "declare ptr @nex_rt_http_method(ptr)",
    "declare ptr @nex_rt_http_path(ptr)",
    "declare ptr @nex_rt_http_version(ptr)",
    "declare ptr @nex_rt_http_header(ptr, ptr)",
    "declare ptr @nex_rt_http_body(ptr)",
    "declare ptr @nex_rt_http_response(ptr, ptr, ptr)",
    "declare i64 @nex_rt_unix_time()",
    "declare i64 @nex_rt_now_ms()",
    "declare void @nex_rt_sleep_ms(i64)",
    "declare ptr @nex_rt_http_date()",
    "declare i64 @nex_rt_str_index_of_from(ptr, ptr, i64)",
    "declare ptr @nex_rt_char_at(ptr, i64)",
    "declare i64 @nex_rt_char_code(ptr)",
    "declare void @nex_rt_global_set_i64(i64, i64)",
    "declare i64 @nex_rt_global_get_i64(i64)",
    "declare void @nex_rt_global_set_f64(i64, double)",
    "declare double @nex_rt_global_get_f64(i64)",
    "declare void @nex_rt_global_set_str(i64, ptr)",
    "declare ptr @nex_rt_global_get_str(i64)",
    "declare i64 @nex_rt_mutex_new()",
    "declare void @nex_rt_mutex_lock(i64)",
    "declare void @nex_rt_mutex_unlock(i64)",
    "declare i64 @nex_rt_thread_spawn(i64)",
    "declare i64 @nex_rt_thread_join(i64)",
    "declare i64 @nex_rt_thread_running(i64)",
    "declare i64 @nex_rt_map_new(i64)",
    "declare i64 @nex_rt_map_set(i64, ptr, ptr)",
    "declare ptr @nex_rt_map_get(i64, ptr)",
    "declare i64 @nex_rt_map_has(i64, ptr)",
    "declare i64 @nex_rt_map_remove(i64, ptr)",
    "declare i64 @nex_rt_map_len(i64)",
    "declare ptr @nex_rt_map_key_at(i64, i64)",
    "declare i64 @nex_rt_str_split_count(ptr, ptr)",
    "declare ptr @nex_rt_str_split_at(ptr, ptr, i64)",
    "declare ptr @nex_rt_json_stringify(i64)",
    "declare i64 @nex_rt_json_parse(ptr)",
    "declare ptr @nex_rt_json_type(i64)",
    "declare i64 @nex_rt_json_len(i64)",
    "declare i64 @nex_rt_json_at(i64, i64)",
    "declare i64 @nex_rt_json_get(i64, ptr)",
    "declare ptr @nex_rt_json_str(i64)",
    "declare double @nex_rt_json_num(i64)",
    "declare i64 @nex_rt_json_bool(i64)",
    "declare i64 @nex_rt_json_is_null(i64)",
    "declare ptr @nex_rt_array_alloc(i64)",
    "declare ptr @nex_rt_array_new()",
    "declare ptr @nex_rt_array_grow(ptr, i64)",
    "declare ptr @nex_rt_array_ensure(ptr, i64, i64)",
    "declare ptr @nex_rt_i64_to_string(i64)",
    "declare i64 @nex_rt_min_i64(i64, i64)",
    "declare i64 @nex_rt_max_i64(i64, i64)",
    "declare double @nex_rt_min_f64(double, double)",
    "declare double @nex_rt_max_f64(double, double)",
    "declare double @nex_rt_sqrt(double)",
    "declare double @nex_rt_pow(double, double)",
    "declare void @nex_rt_panic(ptr) noreturn",
];

pub fn emit_module(module: &IrModule) -> Result<String, String> {
    let mut emitter = Emitter::new(module);
    emitter.emit()
}

struct Emitter<'a> {
    module: &'a IrModule,
    globals: Vec<(String, String)>,
    global_counter: usize,
    string_cache: HashMap<String, String>,
}

impl<'a> Emitter<'a> {
    fn new(module: &'a IrModule) -> Self {
        Self {
            module,
            globals: Vec::new(),
            global_counter: 0,
            string_cache: HashMap::new(),
        }
    }

    fn emit(&mut self) -> Result<String, String> {
        let mut out = String::new();
        out.push_str("; ModuleID = 'nex'\n");
        out.push_str("source_filename = \"nex\"\n\n");

        for definition in &self.module.structs {
            let fields = definition
                .fields
                .iter()
                .map(|ty| llvm_type(*ty))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!(
                "%struct.{} = type {{ {} }}\n",
                definition.name, fields
            ));
        }
        if !self.module.structs.is_empty() {
            out.push('\n');
        }

        let mut body = String::new();
        let mut sig_map: HashMap<String, (Vec<IrType>, IrType)> = HashMap::new();
        for function in &self.module.functions {
            sig_map.insert(
                function.name.clone(),
                (
                    function.params.iter().map(|(_, ty)| *ty).collect(),
                    function.ret,
                ),
            );
        }
        for external in &self.module.externs {
            sig_map.insert(
                external.name.clone(),
                (external.params.clone(), external.ret),
            );
        }

        for external in &self.module.externs {
            let ret = llvm_type(external.ret);
            let params = external
                .params
                .iter()
                .map(|ty| llvm_type(*ty))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!("declare {} @{}({})\n", ret, external.name, params));
        }
        if !self.module.externs.is_empty() {
            out.push('\n');
        }

        for function in &self.module.functions {
            self.emit_function(function, &mut body, &sig_map)?;
        }

        let main_signature = sig_map
            .get("main")
            .ok_or_else(|| "program entry point `main` is required".to_string())?;
        self.emit_main_wrapper(main_signature.1, &mut body)?;

        if !self.globals.is_empty() {
            for (name, definition) in &self.globals {
                out.push_str(&format!(
                    "{name} = private unnamed_addr constant {definition}, align 1\n"
                ));
            }
            out.push('\n');
        }

        for decl in RUNTIME_DECLS {
            out.push_str(decl);
            out.push('\n');
        }
        out.push('\n');
        out.push_str(&body);
        Ok(out)
    }

    fn emit_function(
        &mut self,
        function: &IrFunction,
        body: &mut String,
        sig_map: &HashMap<String, (Vec<IrType>, IrType)>,
    ) -> Result<(), String> {
        let ret = llvm_type(function.ret);
        let params = function
            .params
            .iter()
            .map(|(id, ty)| format!("{} %v{}", llvm_type(*ty), id.0))
            .collect::<Vec<_>>()
            .join(", ");
        body.push_str(&format!(
            "define {} @nex.fn.{}({}) {{\n",
            ret, function.name, params
        ));

        for (index, block) in function.blocks.iter().enumerate() {
            if index > 0 {
                body.push_str(&format!("{}:\n", block_label(block.id)));
            } else {
                body.push_str("entry:\n");
            }

            for inst in &block.insts {
                self.emit_inst(inst, body, sig_map)?;
            }

            match &block.terminator {
                Some(terminator) => {
                    self.emit_terminator(terminator, function.ret, body, sig_map)?
                }
                None => body.push_str("  unreachable\n"),
            }
        }

        body.push_str("}\n\n");
        Ok(())
    }

    fn emit_main_wrapper(&self, main_ret: IrType, body: &mut String) -> Result<(), String> {
        body.push_str("define i32 @main() {\nentry:\n");
        if main_ret == IrType::I32 {
            body.push_str("  %nex.main.result = call i32 @nex.fn.main()\n");
            body.push_str("  ret i32 %nex.main.result\n");
        } else {
            body.push_str("  call void @nex.fn.main()\n");
            body.push_str("  ret i32 0\n");
        }
        body.push_str("}\n");
        Ok(())
    }

    fn emit_inst(
        &mut self,
        inst: &Inst,
        body: &mut String,
        sig_map: &HashMap<String, (Vec<IrType>, IrType)>,
    ) -> Result<(), String> {
        match inst {
            Inst::Alloca { dst, ty } => {
                body.push_str(&format!("  %v{} = alloca {}\n", dst.0, llvm_type(*ty)));
            }
            Inst::Store { ptr, value, ty } => {
                body.push_str(&format!(
                    "  store {} {}, ptr %v{}\n",
                    llvm_type(*ty),
                    self.operand_str(value, *ty)?,
                    ptr.0
                ));
            }
            Inst::Load { dst, ptr, ty } => {
                body.push_str(&format!(
                    "  %v{} = load {}, ptr %v{}\n",
                    dst.0,
                    llvm_type(*ty),
                    ptr.0
                ));
            }
            Inst::FieldPtr {
                dst,
                base,
                struct_name,
                index,
                field_ty: _,
            } => {
                body.push_str(&format!(
                    "  %v{} = getelementptr inbounds %struct.{}, ptr %v{}, i32 0, i32 {}\n",
                    dst.0, struct_name, base.0, index
                ));
            }
            Inst::IndexPtr {
                dst,
                base,
                array_type,
                index,
                elem_ty: _,
            } => {
                let index_str = format!("i64 {}", self.operand_str(index, IrType::I64)?);
                body.push_str(&format!(
                    "  %v{} = getelementptr inbounds {}, ptr %v{}, i64 0, {}\n",
                    dst.0, array_type, base.0, index_str
                ));
            }
            Inst::FuncPtr { dst, name } => {
                body.push_str(&format!("  %v{} = ptrtoint ptr @{} to i64\n", dst.0, name));
            }
            Inst::HeaderPtr { dst, base, index } => {
                body.push_str(&format!(
                    "  %v{} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr %v{}, i32 0, i32 {}\n",
                    dst.0, base.0, index
                ));
            }
            Inst::DynIndexPtr {
                dst,
                base,
                index,
                elem_ty,
            } => {
                let index_str = format!("i64 {}", self.operand_str(index, IrType::I64)?);
                let elem_type = llvm_type(*elem_ty);
                body.push_str(&format!(
                    "  %v{} = getelementptr inbounds {}, ptr %v{}, {}\n",
                    dst.0, elem_type, base.0, index_str
                ));
            }
            Inst::Bin {
                dst,
                op,
                ty,
                lhs,
                rhs,
            } => {
                let instruction = bin_instruction(*op, *ty);
                body.push_str(&format!(
                    "  %v{} = {} {} {}, {}\n",
                    dst.0,
                    instruction,
                    llvm_type(*ty),
                    self.operand_str(lhs, *ty)?,
                    self.operand_str(rhs, *ty)?
                ));
            }
            Inst::Un {
                dst,
                op,
                ty,
                operand,
            } => match op {
                UnOp::Neg => {
                    if ty.is_float() {
                        body.push_str(&format!(
                            "  %v{} = fneg {} {}\n",
                            dst.0,
                            llvm_type(*ty),
                            self.operand_str(operand, *ty)?
                        ));
                    } else {
                        body.push_str(&format!(
                            "  %v{} = sub {} 0, {}\n",
                            dst.0,
                            llvm_type(*ty),
                            self.operand_str(operand, *ty)?
                        ));
                    }
                }
                UnOp::Not => {
                    body.push_str(&format!(
                        "  %v{} = xor i1 {}, true\n",
                        dst.0,
                        self.operand_str(operand, IrType::Bool)?
                    ));
                }
            },
            Inst::Cast {
                dst,
                kind,
                from,
                to,
                operand,
            } => {
                let instruction = match kind {
                    CastKind::SignExtend => "sext",
                    CastKind::ZeroExtend => "zext",
                    CastKind::FloatExtend => "fpext",
                    CastKind::Truncate => "trunc",
                    CastKind::SignedIntToFloat => "sitofp",
                    CastKind::UnsignedIntToFloat => "uitofp",
                    CastKind::FloatToSignedInt => "fptosi",
                    CastKind::FloatToUnsignedInt => "fptoui",
                };
                body.push_str(&format!(
                    "  %v{} = {} {} {} to {}\n",
                    dst.0,
                    instruction,
                    llvm_type(*from),
                    self.operand_str(operand, *from)?,
                    llvm_type(*to)
                ));
            }
            Inst::Call {
                dst,
                name,
                args,
                ret,
            } => {
                let symbol = if name.starts_with("nex_rt_")
                    || self
                        .module
                        .externs
                        .iter()
                        .any(|external| external.name == *name)
                {
                    name.clone()
                } else {
                    format!("nex.fn.{name}")
                };
                let (param_types, return_type) = if name.starts_with("nex_rt_") {
                    runtime_signature(name)
                } else {
                    sig_map
                        .get(name)
                        .cloned()
                        .ok_or_else(|| format!("missing signature for `{name}`"))?
                };
                let mut rendered_args = Vec::new();
                for (arg, ty) in args.iter().zip(param_types.iter()) {
                    rendered_args.push(format!(
                        "{} {}",
                        llvm_type(*ty),
                        self.operand_str(arg, *ty)?
                    ));
                }
                let args = rendered_args.join(", ");
                let ret_ty = if name.starts_with("nex_rt_") {
                    return_type
                } else {
                    *ret
                };
                if let Some(dst) = dst {
                    body.push_str(&format!(
                        "  %v{} = call {} @{}({})\n",
                        dst.0,
                        llvm_type(ret_ty),
                        symbol,
                        args
                    ));
                } else {
                    body.push_str(&format!(
                        "  call {} @{}({})\n",
                        llvm_type(ret_ty),
                        symbol,
                        args
                    ));
                }
            }
        }
        Ok(())
    }

    fn emit_terminator(
        &mut self,
        terminator: &Terminator,
        function_ret: IrType,
        body: &mut String,
        _sig_map: &HashMap<String, (Vec<IrType>, IrType)>,
    ) -> Result<(), String> {
        match terminator {
            Terminator::Ret(Some(operand)) => {
                body.push_str(&format!(
                    "  ret {} {}\n",
                    llvm_type(function_ret),
                    self.operand_str(operand, function_ret)?
                ));
            }
            Terminator::Ret(None) => {
                if function_ret == IrType::Void {
                    body.push_str("  ret void\n");
                } else {
                    return Err("non-void function returned no value".to_string());
                }
            }
            Terminator::Br(target) => {
                body.push_str(&format!("  br label %{}\n", block_label(*target)));
            }
            Terminator::CondBr {
                cond,
                then_bb,
                else_bb,
            } => {
                body.push_str(&format!(
                    "  br i1 {}, label %{}, label %{}\n",
                    self.operand_str(cond, IrType::Bool)?,
                    block_label(*then_bb),
                    block_label(*else_bb)
                ));
            }
            Terminator::Unreachable => {
                body.push_str("  unreachable\n");
            }
        }
        Ok(())
    }

    fn operand_str(&mut self, operand: &Operand, ty: IrType) -> Result<String, String> {
        match operand {
            Operand::Value(value) => Ok(format!("%v{}", value.0)),
            Operand::Const(Const::Bool(value)) => {
                Ok(if *value { "true" } else { "false" }.to_string())
            }
            Operand::Const(Const::Int(value)) => Ok(value.to_string()),
            Operand::Const(Const::Float(value)) => {
                if ty == IrType::F32 {
                    Ok(format!("{:.9e}", *value as f32))
                } else {
                    Ok(format!("{:.17e}", value))
                }
            }
            Operand::Const(Const::Char(value)) => Ok((*value as u32).to_string()),
            Operand::Const(Const::Str(value)) => {
                if let Some(name) = self.string_cache.get(value) {
                    return Ok(name.clone());
                }
                let name = self.global_for(value);
                self.string_cache.insert(value.clone(), name.clone());
                Ok(name)
            }
        }
    }

    fn global_for(&mut self, value: &str) -> String {
        let name = format!("@.str.{}", self.global_counter);
        self.global_counter += 1;

        let mut escaped = String::new();
        for byte in value.bytes() {
            match byte {
                b'"' => escaped.push_str("\\22"),
                b'\\' => escaped.push_str("\\5C"),
                0x20..=0x7e => escaped.push(byte as char),
                _ => escaped.push_str(&format!("\\{:02X}", byte)),
            }
        }
        escaped.push_str("\\00");
        let definition = format!("[{} x i8] c\"{}\"", value.len() + 1, escaped);
        self.globals.push((name.clone(), definition));
        name
    }
}

fn block_label(id: BlockId) -> String {
    if id.0 == 0 {
        "entry".to_string()
    } else {
        format!("bb{}", id.0)
    }
}

fn llvm_type(ty: IrType) -> String {
    match ty {
        IrType::Bool => "i1".to_string(),
        IrType::Char => "i32".to_string(),
        IrType::I8 => "i8".to_string(),
        IrType::I16 => "i16".to_string(),
        IrType::I32 => "i32".to_string(),
        IrType::I64 => "i64".to_string(),
        IrType::U8 => "i8".to_string(),
        IrType::U16 => "i16".to_string(),
        IrType::U32 => "i32".to_string(),
        IrType::U64 => "i64".to_string(),
        IrType::F32 => "float".to_string(),
        IrType::F64 => "double".to_string(),
        IrType::String => "ptr".to_string(),
        IrType::Struct(name) => format!("%struct.{name}"),
        IrType::Array(llvm) => llvm.to_string(),
        IrType::Void => "void".to_string(),
    }
}

fn bin_instruction(op: BinOp, ty: IrType) -> &'static str {
    let float = ty.is_float();
    let unsigned = ty.is_unsigned();
    match (op, float, unsigned) {
        (BinOp::Add, true, _) => "fadd",
        (BinOp::Add, false, _) => "add",
        (BinOp::Sub, true, _) => "fsub",
        (BinOp::Sub, false, _) => "sub",
        (BinOp::Mul, true, _) => "fmul",
        (BinOp::Mul, false, _) => "mul",
        (BinOp::Div, true, _) => "fdiv",
        (BinOp::Div, false, true) => "udiv",
        (BinOp::Div, false, false) => "sdiv",
        (BinOp::Rem, true, _) => "frem",
        (BinOp::Rem, false, true) => "urem",
        (BinOp::Rem, false, false) => "srem",
        (BinOp::Eq, true, _) => "fcmp oeq",
        (BinOp::Eq, false, _) => "icmp eq",
        (BinOp::Ne, true, _) => "fcmp one",
        (BinOp::Ne, false, _) => "icmp ne",
        (BinOp::Lt, true, _) => "fcmp olt",
        (BinOp::Lt, false, true) => "icmp ult",
        (BinOp::Lt, false, false) => "icmp slt",
        (BinOp::Le, true, _) => "fcmp ole",
        (BinOp::Le, false, true) => "icmp ule",
        (BinOp::Le, false, false) => "icmp sle",
        (BinOp::Gt, true, _) => "fcmp ogt",
        (BinOp::Gt, false, true) => "icmp ugt",
        (BinOp::Gt, false, false) => "icmp sgt",
        (BinOp::Ge, true, _) => "fcmp oge",
        (BinOp::Ge, false, true) => "icmp uge",
        (BinOp::Ge, false, false) => "icmp sge",
    }
}

fn runtime_signature(name: &str) -> (Vec<IrType>, IrType) {
    let void = IrType::Void;
    let i64 = IrType::I64;
    let i32 = IrType::I32;
    let f64 = IrType::F64;
    let ptr = IrType::String;

    match name {
        "nex_rt_print_str" | "nex_rt_println_str" => (vec![ptr], void),
        "nex_rt_print_i64" | "nex_rt_println_i64" => (vec![i64], void),
        "nex_rt_print_u64" | "nex_rt_println_u64" => (vec![i64], void),
        "nex_rt_print_f64" | "nex_rt_println_f64" => (vec![f64], void),
        "nex_rt_print_bool" | "nex_rt_println_bool" => (vec![i32], void),
        "nex_rt_print_char" | "nex_rt_println_char" => (vec![i32], void),
        "nex_rt_str_eq" => (vec![ptr, ptr], i32),
        "nex_rt_str_cmp" => (vec![ptr, ptr], i32),
        "nex_rt_len" => (vec![ptr], i64),
        "nex_rt_abs" => (vec![i64], i64),
        "nex_rt_alloc" => (vec![i64], i64),
        "nex_rt_free" => (vec![i64], void),
        "nex_rt_peek_i64" => (vec![i64], i64),
        "nex_rt_poke_i64" => (vec![i64, i64], void),
        "nex_rt_str_concat" => (vec![ptr, ptr], ptr),
        "nex_rt_str_contains" => (vec![ptr, ptr], i32),
        "nex_rt_str_index_of" => (vec![ptr, ptr], i64),
        "nex_rt_str_substring" => (vec![ptr, i64, i64], ptr),
        "nex_rt_str_upper" | "nex_rt_str_lower" | "nex_rt_str_trim" => (vec![ptr], ptr),
        "nex_rt_str_starts_with" | "nex_rt_str_ends_with" => (vec![ptr, ptr], i32),
        "nex_rt_str_repeat" => (vec![ptr, i64], ptr),
        "nex_rt_str_replace" => (vec![ptr, ptr, ptr], ptr),
        "nex_rt_str_reverse" => (vec![ptr], ptr),
        "nex_rt_parse_i64" => (vec![ptr], i64),
        "nex_rt_abs_f64" => (vec![f64], f64),
        "nex_rt_f64_to_string" => (vec![f64], ptr),
        "nex_rt_bool_to_string" => (vec![i32], ptr),
        "nex_rt_read_file" => (vec![ptr], ptr),
        "nex_rt_write_file" => (vec![ptr, ptr], i32),
        "nex_rt_file_exists" => (vec![ptr], i32),
        "nex_rt_file_size" => (vec![ptr], i64),
        "nex_rt_tcp_connect" => (vec![ptr, i64], i64),
        "nex_rt_tcp_listen" => (vec![i64, i64], i64),
        "nex_rt_tcp_accept" => (vec![i64], i64),
        "nex_rt_tcp_write" => (vec![i64, ptr, i64], i64),
        "nex_rt_tcp_read" => (vec![i64, i64], ptr),
        "nex_rt_tcp_close" => (vec![i64], i32),
        "nex_rt_http_method" | "nex_rt_http_path" | "nex_rt_http_version" => (vec![ptr], ptr),
        "nex_rt_http_header" => (vec![ptr, ptr], ptr),
        "nex_rt_http_body" => (vec![ptr], ptr),
        "nex_rt_http_response" => (vec![ptr, ptr, ptr], ptr),
        "nex_rt_unix_time" | "nex_rt_now_ms" => (vec![], i64),
        "nex_rt_sleep_ms" => (vec![i64], void),
        "nex_rt_http_date" => (vec![], ptr),
        "nex_rt_str_index_of_from" => (vec![ptr, ptr, i64], i64),
        "nex_rt_char_at" => (vec![ptr, i64], ptr),
        "nex_rt_char_code" => (vec![ptr], i64),
        "nex_rt_global_set_i64" => (vec![i64, i64], void),
        "nex_rt_global_get_i64" => (vec![i64], i64),
        "nex_rt_global_set_f64" => (vec![i64, f64], void),
        "nex_rt_global_get_f64" => (vec![i64], f64),
        "nex_rt_global_set_str" => (vec![i64, ptr], void),
        "nex_rt_global_get_str" => (vec![i64], ptr),
        "nex_rt_mutex_new" => (vec![], i64),
        "nex_rt_mutex_lock" => (vec![i64], void),
        "nex_rt_mutex_unlock" => (vec![i64], void),
        "nex_rt_thread_spawn" => (vec![i64], i64),
        "nex_rt_thread_join" => (vec![i64], i64),
        "nex_rt_thread_running" => (vec![i64], i64),
        "nex_rt_map_new" => (vec![i64], i64),
        "nex_rt_map_set" => (vec![i64, ptr, ptr], i64),
        "nex_rt_map_get" => (vec![i64, ptr], ptr),
        "nex_rt_map_has" => (vec![i64, ptr], i64),
        "nex_rt_map_remove" => (vec![i64, ptr], i64),
        "nex_rt_map_len" => (vec![i64], i64),
        "nex_rt_map_key_at" => (vec![i64, i64], ptr),
        "nex_rt_str_split_count" => (vec![ptr, ptr], i64),
        "nex_rt_str_split_at" => (vec![ptr, ptr, i64], ptr),
        "nex_rt_json_stringify" => (vec![i64], ptr),
        "nex_rt_json_parse" => (vec![ptr], i64),
        "nex_rt_json_type" => (vec![i64], ptr),
        "nex_rt_json_len" => (vec![i64], i64),
        "nex_rt_json_at" => (vec![i64, i64], i64),
        "nex_rt_json_get" => (vec![i64, ptr], i64),
        "nex_rt_json_str" => (vec![i64], ptr),
        "nex_rt_json_num" => (vec![i64], f64),
        "nex_rt_json_bool" | "nex_rt_json_is_null" => (vec![i64], i64),
        "nex_rt_array_alloc" => (vec![i64], ptr),
        "nex_rt_array_new" => (vec![], ptr),
        "nex_rt_array_grow" => (vec![ptr, i64], ptr),
        "nex_rt_array_ensure" => (vec![ptr, i64, i64], ptr),
        "nex_rt_i64_to_string" => (vec![i64], ptr),
        "nex_rt_min_i64" | "nex_rt_max_i64" => (vec![i64, i64], i64),
        "nex_rt_min_f64" | "nex_rt_max_f64" => (vec![f64, f64], f64),
        "nex_rt_sqrt" => (vec![f64], f64),
        "nex_rt_pow" => (vec![f64, f64], f64),
        "nex_rt_panic" => (vec![ptr], void),
        _ => (vec![], void),
    }
}
