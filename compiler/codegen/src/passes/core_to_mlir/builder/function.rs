use liblumen_diagnostics::SourceSpan;
use liblumen_mlir::*;
use liblumen_number::Integer;
use liblumen_syntax_core::{self as syntax_core, ir::instructions::*, DataFlowGraph};
use liblumen_syntax_core::{ConstantItem, Immediate, TermType};

use log::debug;

use super::*;

impl<'m> ModuleBuilder<'m> {
    /// Lowers the definition of a syntax_core function to CIR dialect
    pub(super) fn build_function(
        &mut self,
        function: &syntax_core::Function,
    ) -> anyhow::Result<()> {
        debug!("building mlir function {}", function.signature.mfa());
        // Reset the block/value maps for this function
        self.blocks.clear();
        self.values.clear();
        // Declare the function
        let function_loc = self.location_from_span(function.span);
        let name = function.signature.mfa().to_string();
        let func = self.builder.get_func_by_symbol(name.as_str()).unwrap();
        let function_body = func.get_region(0);
        let mut tx_param_types = Vec::with_capacity(8);
        let mut tx_param_locs = Vec::with_capacity(8);
        // Build lookup map for syntax_core blocks to MLIR blocks, creating the blocks in the process
        {
            let builder = CirBuilder::new(&self.builder);
            self.blocks.extend(function.dfg.blocks().map(|(b, _data)| {
                tx_param_types.clear();
                tx_param_locs.clear();
                let orig_param_types = function.dfg.block_param_types(b);
                for t in orig_param_types.iter() {
                    tx_param_types.push(translate_ir_type(
                        &self.module,
                        &self.options,
                        &builder,
                        t,
                    ));
                    tx_param_locs.push(function_loc);
                }
                // Create the corresponding MLIR block
                let mlir_block = builder.create_block_in_region(
                    function_body,
                    tx_param_types.as_slice(),
                    tx_param_locs.as_slice(),
                );
                // Map all syntax_core block parameters to their MLIR block argument values
                for (value, mlir_value) in function
                    .dfg
                    .block_params(b)
                    .iter()
                    .zip(mlir_block.arguments())
                {
                    self.values.insert(*value, mlir_value.base());
                }
                (b, mlir_block)
            }));
        }
        // For each block, in layout order, fill out the block with translated instructions
        for (block, block_data) in function.dfg.blocks() {
            self.switch_to_block(block);
            for inst in block_data.insts() {
                self.build_inst(&function.dfg, inst)?;
            }
        }

        Ok(())
    }

    /// Switches the builder to the MLIR block corresponding to the given syntax_core block
    fn switch_to_block(&mut self, block: syntax_core::Block) {
        debug!("switching builder to block {:?}", block);
        self.current_source_block = block;
        self.current_block = self.blocks[&block];
        self.builder.set_insertion_point_to_end(self.current_block);
    }

    /// Lowers the declaration of a syntax_core function to CIR dialect
    pub(super) fn declare_function(
        &self,
        span: SourceSpan,
        sig: &syntax_core::Signature,
    ) -> anyhow::Result<FuncOp> {
        debug!("declaring function {}", sig.mfa());
        // Generate the symbol name for the function, e.g. module:function/arity
        let name = sig.mfa().to_string();
        let builder = self.cir();
        let ty = signature_to_fn_type(self.module, self.options, &builder, &sig);
        let vis = if sig.visibility.is_public() && !sig.visibility.is_externally_defined() {
            Visibility::Public
        } else {
            Visibility::Private
        };
        let _ip = self.builder.insertion_guard();
        self.builder
            .set_insertion_point_to_end(self.mlir_module.body());
        let loc = self.location_from_span(span);
        let op = self.builder.build_func(loc, name.as_str(), ty, &[], &[]);
        op.set_visibility(vis);
        Ok(op)
    }

    fn immediate_to_constant(&self, loc: Location, imm: Immediate) -> ValueBase {
        let builder = CirBuilder::new(&self.builder);
        match imm {
            Immediate::Bool(b) => {
                let op = builder.build_constant(
                    loc,
                    builder.get_cir_bool_type(),
                    builder.get_bool_attr(b),
                );
                op.get_result(0).base()
            }
            Immediate::Atom(a) => {
                let ty = builder.get_cir_atom_type();
                let op = builder.build_constant(loc, ty, builder.get_atom_attr(a, ty));
                op.get_result(0).base()
            }
            Immediate::Integer(i) => {
                let op = builder.build_constant(
                    loc,
                    builder.get_cir_isize_type(),
                    builder.get_isize_attr(i.try_into().unwrap()),
                );
                op.get_result(0).base()
            }
            Immediate::Float(f) => {
                let op = builder.build_constant(
                    loc,
                    builder.get_cir_float_type(),
                    builder.get_float_attr(f),
                );
                op.get_result(0).base()
            }
            Immediate::Nil => {
                let ty = builder.get_cir_nil_type();
                let op = builder.build_constant(loc, ty, builder.get_nil_attr());
                op.get_result(0).base()
            }
            Immediate::None => {
                let ty = builder.get_cir_none_type();
                let op = builder.build_constant(loc, ty, builder.get_none_attr());
                op.get_result(0).base()
            }
        }
    }

    fn const_to_constant(&self, loc: Location, constant: &ConstantItem) -> ValueBase {
        let builder = CirBuilder::new(&self.builder);
        match constant {
            ConstantItem::Integer(Integer::Small(i)) => {
                let op = builder.build_constant(
                    loc,
                    builder.get_cir_isize_type(),
                    builder.get_isize_attr((*i).try_into().unwrap()),
                );
                op.get_result(0).base()
            }
            ConstantItem::Integer(Integer::Big(_)) => todo!("bigint constants"),
            ConstantItem::Float(f) => {
                let op = builder.build_constant(
                    loc,
                    builder.get_cir_float_type(),
                    builder.get_float_attr(*f),
                );
                op.get_result(0).base()
            }
            ConstantItem::Bool(b) => {
                let op = builder.build_constant(
                    loc,
                    builder.get_cir_bool_type(),
                    builder.get_bool_attr(*b),
                );
                op.get_result(0).base()
            }
            ConstantItem::Atom(a) => {
                let ty = builder.get_cir_atom_type();
                let op = builder.build_constant(loc, ty, builder.get_atom_attr(*a, ty));
                op.get_result(0).base()
            }
            ConstantItem::Binary(_const_data) => todo!("binary constants"),
            ConstantItem::Tuple(_elements) => todo!("tuple constants"),
            ConstantItem::List(_elements) => todo!("list constants"),
            ConstantItem::Map(_elements) => todo!("map constants"),
        }
    }

    /// Lowers a single syntax_core instruction to the corresponding CIR dialect operation
    fn build_inst(&mut self, dfg: &DataFlowGraph, inst: Inst) -> anyhow::Result<()> {
        let inst_data = &dfg[inst];
        let inst_span = inst_data.span();
        debug!(
            "translating instruction with opcode {:?} to mlir",
            inst_data.opcode()
        );
        match inst_data.as_ref() {
            InstData::UnaryOp(op) => self.build_unary_op(dfg, inst, inst_span, op),
            InstData::UnaryOpImm(op) => self.build_unary_op_imm(dfg, inst, inst_span, op),
            InstData::UnaryOpConst(op) => self.build_unary_op_const(dfg, inst, inst_span, op),
            InstData::BinaryOp(op) => self.build_binary_op(dfg, inst, inst_span, op),
            InstData::BinaryOpImm(op) => self.build_binary_op_imm(dfg, inst, inst_span, op),
            InstData::BinaryOpConst(op) => self.build_binary_op_const(dfg, inst, inst_span, op),
            InstData::Ret(op) => self.build_ret(dfg, inst, inst_span, op),
            InstData::RetImm(op) => self.build_ret_imm(dfg, inst, inst_span, op),
            InstData::Br(op) => self.build_br(dfg, inst, inst_span, op),
            InstData::IsType(op) => self.build_is_type(dfg, inst, inst_span, op),
            InstData::PrimOp(op) => self.build_primop(dfg, inst, inst_span, op),
            InstData::PrimOpImm(op) => self.build_primop_imm(dfg, inst, inst_span, op),
            InstData::Call(op) => self.build_call(dfg, inst, inst_span, op),
            InstData::CallIndirect(op) => self.build_call_indirect(dfg, inst, inst_span, op),
            InstData::SetElement(op) => self.build_setelement(dfg, inst, inst_span, op),
            InstData::SetElementImm(op) => self.build_setelement_imm(dfg, inst, inst_span, op),
            InstData::SetElementConst(op) => self.build_setelement_const(dfg, inst, inst_span, op),
            other => unimplemented!("{:?}", other),
        }
    }

    fn build_unary_op(
        &mut self,
        dfg: &DataFlowGraph,
        inst: Inst,
        span: SourceSpan,
        op: &UnaryOp,
    ) -> anyhow::Result<()> {
        let loc = self.location_from_span(span);
        let arg = self.values[&op.arg];
        let mlir_op = match op.op {
            Opcode::IsNull => self.cir().build_is_null(loc, arg).base(),
            Opcode::Head => self.cir().build_head(loc, arg).base(),
            Opcode::Tail => self.cir().build_tail(loc, arg).base(),
            Opcode::Neg => {
                let neg1 = self.get_or_declare_function("erlang:-/1").unwrap();
                self.cir().build_call(loc, neg1, &[arg]).base()
            }
            Opcode::Not => self.cir().build_not(loc, arg).base(),
            Opcode::Bnot => {
                let bnot1 = self.get_or_declare_function("erlang:bnot/1").unwrap();
                self.cir().build_call(loc, bnot1, &[arg]).base()
            }
            other => unimplemented!("no lowering for unary op with opcode {:?}", other),
        };

        let results = dfg.inst_results(inst);
        for (value, op_result) in results.iter().copied().zip(mlir_op.results()) {
            self.values.insert(value, op_result.base());
        }
        Ok(())
    }

    fn build_unary_op_imm(
        &mut self,
        dfg: &DataFlowGraph,
        inst: Inst,
        span: SourceSpan,
        op: &UnaryOpImm,
    ) -> anyhow::Result<()> {
        let loc = self.location_from_span(span);
        let mlir_op = match op.op {
            Opcode::ImmNull => {
                let builder = self.cir();
                let result = dfg.first_result(inst);
                let ty =
                    translate_ir_type(self.module, self.options, &builder, &dfg.value_type(result));
                let null = builder.build_null(loc, ty);
                self.values.insert(result, null.get_result(0).base());
                return Ok(());
            }
            Opcode::ImmInt
            | Opcode::ImmFloat
            | Opcode::ImmBool
            | Opcode::ImmAtom
            | Opcode::ImmNil
            | Opcode::ImmNone => {
                let imm = self.immediate_to_constant(loc, op.imm);
                self.values.insert(dfg.first_result(inst), imm);
                return Ok(());
            }
            Opcode::Tuple => match op.imm {
                Immediate::Integer(arity) => self
                    .cir()
                    .build_tuple(loc, arity.try_into().unwrap())
                    .base(),
                other => panic!(
                    "invalid tuple op, only integer immediates are allowed, got {:?}",
                    other
                ),
            },
            Opcode::Neg => {
                let imm = self.immediate_to_constant(loc, op.imm);
                let neg1 = self.get_or_declare_function("erlang:-/1").unwrap();
                self.cir().build_call(loc, neg1, &[imm]).base()
            }
            Opcode::Not => {
                let imm = self.immediate_to_constant(loc, op.imm);
                self.cir().build_not(loc, imm).base()
            }
            Opcode::Bnot => {
                let imm = self.immediate_to_constant(loc, op.imm);
                let bnot1 = self.get_or_declare_function("erlang:bnot/1").unwrap();
                self.cir().build_call(loc, bnot1, &[imm]).base()
            }
            other => unimplemented!("no lowering for unary op immediate with opcode {}", other),
        };

        let results = dfg.inst_results(inst);
        for (value, op_result) in results.iter().copied().zip(mlir_op.results()) {
            self.values.insert(value, op_result.base());
        }
        Ok(())
    }

    fn build_unary_op_const(
        &mut self,
        dfg: &DataFlowGraph,
        inst: Inst,
        span: SourceSpan,
        op: &UnaryOpConst,
    ) -> anyhow::Result<()> {
        let loc = self.location_from_span(span);
        let imm = self.const_to_constant(loc, &dfg.constant(op.imm));
        let mlir_op = match op.op {
            Opcode::ConstBigInt
            | Opcode::ConstBinary
            | Opcode::ConstTuple
            | Opcode::ConstList
            | Opcode::ConstMap => {
                self.values.insert(dfg.first_result(inst), imm);
                return Ok(());
            }
            Opcode::Neg => {
                let neg1 = self.get_or_declare_function("erlang:-/1").unwrap();
                self.cir().build_call(loc, neg1, &[imm]).base()
            }
            Opcode::Not => self.cir().build_not(loc, imm).base(),
            Opcode::Bnot => {
                let bnot1 = self.get_or_declare_function("erlang:bnot/1").unwrap();
                self.cir().build_call(loc, bnot1, &[imm]).base()
            }
            other => unimplemented!("no lowering for unary op constant with opcode {}", other),
        };

        let results = dfg.inst_results(inst);
        for (value, op_result) in results.iter().copied().zip(mlir_op.results()) {
            self.values.insert(value, op_result.base());
        }
        Ok(())
    }

    fn build_binary_op(
        &mut self,
        dfg: &DataFlowGraph,
        inst: Inst,
        span: SourceSpan,
        op: &BinaryOp,
    ) -> anyhow::Result<()> {
        let loc = self.location_from_span(span);
        let lhs = self.values[&op.args[0]];
        let rhs = self.values[&op.args[1]];
        let mlir_op = match op.op {
            Opcode::Cons => self.cir().build_cons(loc, lhs, rhs).base(),
            Opcode::GetElement => self.cir().build_get_element(loc, lhs, rhs).base(),
            Opcode::ListConcat => {
                let callee = self.get_or_declare_function("erlang:++/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::ListSubtract => {
                let callee = self.get_or_declare_function("erlang:--/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Eq => {
                let callee = self.get_or_declare_function("erlang:==/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::EqExact => {
                let callee = self.get_or_declare_function("erlang:=:=/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Neq => {
                let callee = self.get_or_declare_function("erlang:/=/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::NeqExact => {
                let callee = self.get_or_declare_function("erlang:=/=/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Gt => {
                let callee = self.get_or_declare_function("erlang:>/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Gte => {
                let callee = self.get_or_declare_function("erlang:>=/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Lt => {
                let callee = self.get_or_declare_function("erlang:</2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Lte => {
                let callee = self.get_or_declare_function("erlang:=</2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::And => self.cir().build_and(loc, lhs, rhs).base(),
            Opcode::AndAlso => self.cir().build_andalso(loc, lhs, rhs).base(),
            Opcode::Or => self.cir().build_or(loc, lhs, rhs).base(),
            Opcode::OrElse => self.cir().build_orelse(loc, lhs, rhs).base(),
            Opcode::Xor => self.cir().build_xor(loc, lhs, rhs).base(),
            Opcode::Band => {
                let callee = self.get_or_declare_function("erlang:band/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Bor => {
                let callee = self.get_or_declare_function("erlang:bor/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Bxor => {
                let callee = self.get_or_declare_function("erlang:bxor/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Bsl => {
                let callee = self.get_or_declare_function("erlang:bsl/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Bsr => {
                let callee = self.get_or_declare_function("erlang:bsr/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Div => {
                let callee = self.get_or_declare_function("erlang:div/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Rem => {
                let callee = self.get_or_declare_function("erlang:rem/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Add => {
                let callee = self.get_or_declare_function("erlang:+/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Sub => {
                let callee = self.get_or_declare_function("erlang:-/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Mul => {
                let callee = self.get_or_declare_function("erlang:*/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Fdiv => {
                let callee = self.get_or_declare_function("erlang:fdiv/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            other => unimplemented!("no lowering for binary op with opcode {}", other),
        };

        let results = dfg.inst_results(inst);
        for (value, op_result) in results.iter().copied().zip(mlir_op.results()) {
            self.values.insert(value, op_result.base());
        }
        Ok(())
    }

    fn build_binary_op_imm(
        &mut self,
        dfg: &DataFlowGraph,
        inst: Inst,
        span: SourceSpan,
        op: &BinaryOpImm,
    ) -> anyhow::Result<()> {
        let loc = self.location_from_span(span);
        let lhs = self.values[&op.arg];
        let mlir_op = match op.op {
            Opcode::Cons => {
                let rhs = self.immediate_to_constant(loc, op.imm);
                self.cir().build_cons(loc, lhs, rhs).base()
            }
            Opcode::GetElement => {
                match op.imm {
                    Immediate::Integer(_) => {
                        let rhs = self.immediate_to_constant(loc, op.imm);
                        self.cir().build_get_element(loc, lhs, rhs).base()
                    }
                    _ => panic!("invalid get_element binary immediate op, only integer immediates are supported"),
                }
            }
            Opcode::IsTaggedTuple => {
                match op.imm {
                    Immediate::Atom(a) => self.cir().build_is_tagged_tuple(loc, lhs, a).base(),
                    _ => panic!("invalid is_tagged_tuple binary immediate op, only atom immediates are supported"),
                }
            }
            Opcode::Eq => {
                let rhs = self.immediate_to_constant(loc, op.imm);
                let callee = self.get_or_declare_function("erlang:==/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::EqExact => {
                let rhs = self.immediate_to_constant(loc, op.imm);
                let callee = self.get_or_declare_function("erlang:=:=/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Neq => {
                let rhs = self.immediate_to_constant(loc, op.imm);
                let callee = self.get_or_declare_function("erlang:/=/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::NeqExact => {
                let rhs = self.immediate_to_constant(loc, op.imm);
                let callee = self.get_or_declare_function("erlang:=/=/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Gt => {
                let rhs = self.immediate_to_constant(loc, op.imm);
                let callee = self.get_or_declare_function("erlang:>/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Gte => {
                let rhs = self.immediate_to_constant(loc, op.imm);
                let callee = self.get_or_declare_function("erlang:>=/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Lt => {
                let rhs = self.immediate_to_constant(loc, op.imm);
                let callee = self.get_or_declare_function("erlang:</2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Lte => {
                let rhs = self.immediate_to_constant(loc, op.imm);
                let callee = self.get_or_declare_function("erlang:=</2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::And => {
                match op.imm {
                    Immediate::Bool(_) => self.cir().build_and(loc, lhs, self.immediate_to_constant(loc, op.imm)).base(),
                    _ => panic!("invalid binary immediate op, and requires a boolean immediate"),
                }
            }
            Opcode::AndAlso => {
                match op.imm {
                    Immediate::Bool(_) => self.cir().build_andalso(loc, lhs, self.immediate_to_constant(loc, op.imm)).base(),
                    _ => panic!("invalid binary immediate op, andalso requires a boolean immediate"),
                }
            }
            Opcode::Or => {
                match op.imm {
                    Immediate::Bool(_) => self.cir().build_or(loc, lhs, self.immediate_to_constant(loc, op.imm)).base(),
                    _ => panic!("invalid binary immediate op, or requires a boolean immediate"),
                }
            }
            Opcode::OrElse => {
                match op.imm {
                    Immediate::Bool(_) => self.cir().build_orelse(loc, lhs, self.immediate_to_constant(loc, op.imm)).base(),
                    _ => panic!("invalid binary immediate op, orelse requires a boolean immediate"),
                }
            }
            Opcode::Xor => {
                match op.imm {
                    Immediate::Bool(_) => self.cir().build_xor(loc, lhs, self.immediate_to_constant(loc, op.imm)).base(),
                    _ => panic!("invalid binary immediate op, xor requires a boolean immediate"),
                }
            }
            Opcode::Band => {
                let rhs = self.immediate_to_constant(loc, op.imm);
                let callee = self.get_or_declare_function("erlang:band/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Bor => {
                let rhs = self.immediate_to_constant(loc, op.imm);
                let callee = self.get_or_declare_function("erlang:bor/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Bxor => {
                let rhs = self.immediate_to_constant(loc, op.imm);
                let callee = self.get_or_declare_function("erlang:bxor/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Bsl => {
                let rhs = self.immediate_to_constant(loc, op.imm);
                let callee = self.get_or_declare_function("erlang:bsl/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Bsr => {
                let rhs = self.immediate_to_constant(loc, op.imm);
                let callee = self.get_or_declare_function("erlang:bsr/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Div => {
                let rhs = self.immediate_to_constant(loc, op.imm);
                let callee = self.get_or_declare_function("erlang:div/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Rem => {
                let rhs = self.immediate_to_constant(loc, op.imm);
                let callee = self.get_or_declare_function("erlang:rem/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Add => {
                let rhs = self.immediate_to_constant(loc, op.imm);
                let callee = self.get_or_declare_function("erlang:+/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Sub => {
                let rhs = self.immediate_to_constant(loc, op.imm);
                let callee = self.get_or_declare_function("erlang:-/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Mul => {
                let rhs = self.immediate_to_constant(loc, op.imm);
                let callee = self.get_or_declare_function("erlang:*/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Fdiv => {
                let rhs = self.immediate_to_constant(loc, op.imm);
                let callee = self.get_or_declare_function("erlang:fdiv/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            other => unimplemented!("no lowering for binary immediate op with opcode {}", other),
        };

        let results = dfg.inst_results(inst);
        for (value, op_result) in results.iter().copied().zip(mlir_op.results()) {
            self.values.insert(value, op_result.base());
        }
        Ok(())
    }

    fn build_binary_op_const(
        &mut self,
        dfg: &DataFlowGraph,
        inst: Inst,
        span: SourceSpan,
        op: &BinaryOpConst,
    ) -> anyhow::Result<()> {
        let loc = self.location_from_span(span);
        let lhs = self.values[&op.arg];
        let mlir_op = match op.op {
            Opcode::Cons => {
                let rhs = self.const_to_constant(loc, &dfg.constant(op.imm));
                self.cir().build_cons(loc, lhs, rhs).base()
            }
            Opcode::GetElement => {
                match dfg.constant_type(op.imm) {
                    syntax_core::Type::Term(TermType::Integer) => {
                        let rhs = self.const_to_constant(loc, &dfg.constant(op.imm));
                        self.cir().build_get_element(loc, lhs, rhs).base()
                    }
                    _ => panic!("invalid get_element binary constant op, only small integer constants are supported"),
                }
            }
            Opcode::IsTaggedTuple => {
                match *dfg.constant(op.imm) {
                    ConstantItem::Atom(a) => self.cir().build_is_tagged_tuple(loc, lhs, a).base(),
                    _ => panic!("invalid is_tagged_tuple binary constant op, only atom constant are supported"),
                }
            }
            Opcode::Eq => {
                let rhs = self.const_to_constant(loc, &dfg.constant(op.imm));
                let callee = self.get_or_declare_function("erlang:==/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::EqExact => {
                let rhs = self.const_to_constant(loc, &dfg.constant(op.imm));
                let callee = self.get_or_declare_function("erlang:=:=/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Neq => {
                let rhs = self.const_to_constant(loc, &dfg.constant(op.imm));
                let callee = self.get_or_declare_function("erlang:/=/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::NeqExact => {
                let rhs = self.const_to_constant(loc, &dfg.constant(op.imm));
                let callee = self.get_or_declare_function("erlang:=/=/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Gt => {
                let rhs = self.const_to_constant(loc, &dfg.constant(op.imm));
                let callee = self.get_or_declare_function("erlang:>/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Gte => {
                let rhs = self.const_to_constant(loc, &dfg.constant(op.imm));
                let callee = self.get_or_declare_function("erlang:>=/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Lt => {
                let rhs = self.const_to_constant(loc, &dfg.constant(op.imm));
                let callee = self.get_or_declare_function("erlang:</2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Lte => {
                let rhs = self.const_to_constant(loc, &dfg.constant(op.imm));
                let callee = self.get_or_declare_function("erlang:=</2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::And => {
                match dfg.constant_type(op.imm) {
                    syntax_core::Type::Term(TermType::Bool) => self.cir().build_and(loc, lhs, self.const_to_constant(loc, &dfg.constant(op.imm))).base(),
                    _ => panic!("invalid binary constant op, and requires a boolean constant"),
                }
            }
            Opcode::AndAlso => {
                match dfg.constant_type(op.imm) {
                    syntax_core::Type::Term(TermType::Bool) => self.cir().build_andalso(loc, lhs, self.const_to_constant(loc, &dfg.constant(op.imm))).base(),
                    _ => panic!("invalid binary constant op, andalso requires a boolean constant"),
                }
            }
            Opcode::Or => {
                match dfg.constant_type(op.imm) {
                    syntax_core::Type::Term(TermType::Bool) => self.cir().build_or(loc, lhs, self.const_to_constant(loc, &dfg.constant(op.imm))).base(),
                    _ => panic!("invalid binary constant op, or requires a boolean constant"),
                }
            }
            Opcode::OrElse => {
                match dfg.constant_type(op.imm) {
                    syntax_core::Type::Term(TermType::Bool) => self.cir().build_orelse(loc, lhs, self.const_to_constant(loc, &dfg.constant(op.imm))).base(),
                    _ => panic!("invalid binary constant op, orelse requires a boolean constant"),
                }
            }
            Opcode::Xor => {
                match dfg.constant_type(op.imm) {
                    syntax_core::Type::Term(TermType::Bool) => self.cir().build_xor(loc, lhs, self.const_to_constant(loc, &dfg.constant(op.imm))).base(),
                    _ => panic!("invalid binary constant op, xor requires a boolean constant"),
                }
            }
            Opcode::Band => {
                let rhs = self.const_to_constant(loc, &dfg.constant(op.imm));
                let callee = self.get_or_declare_function("erlang:band/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Bor => {
                let rhs = self.const_to_constant(loc, &dfg.constant(op.imm));
                let callee = self.get_or_declare_function("erlang:bor/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Bxor => {
                let rhs = self.const_to_constant(loc, &dfg.constant(op.imm));
                let callee = self.get_or_declare_function("erlang:bxor/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Bsl => {
                let rhs = self.const_to_constant(loc, &dfg.constant(op.imm));
                let callee = self.get_or_declare_function("erlang:bsl/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Bsr => {
                let rhs = self.const_to_constant(loc, &dfg.constant(op.imm));
                let callee = self.get_or_declare_function("erlang:bsr/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Div => {
                let rhs = self.const_to_constant(loc, &dfg.constant(op.imm));
                let callee = self.get_or_declare_function("erlang:div/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Rem => {
                let rhs = self.const_to_constant(loc, &dfg.constant(op.imm));
                let callee = self.get_or_declare_function("erlang:rem/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Add => {
                let rhs = self.const_to_constant(loc, &dfg.constant(op.imm));
                let callee = self.get_or_declare_function("erlang:+/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Sub => {
                let rhs = self.const_to_constant(loc, &dfg.constant(op.imm));
                let callee = self.get_or_declare_function("erlang:-/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Mul => {
                let rhs = self.const_to_constant(loc, &dfg.constant(op.imm));
                let callee = self.get_or_declare_function("erlang:*/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            Opcode::Fdiv => {
                let rhs = self.const_to_constant(loc, &dfg.constant(op.imm));
                let callee = self.get_or_declare_function("erlang:fdiv/2").unwrap();
                self.cir().build_call(loc, callee, &[lhs, rhs]).base()
            }
            other => unimplemented!("no lowering for binary constant op with opcode {}", other),
        };

        let results = dfg.inst_results(inst);
        for (value, op_result) in results.iter().copied().zip(mlir_op.results()) {
            self.values.insert(value, op_result.base());
        }
        Ok(())
    }

    fn build_ret(
        &self,
        dfg: &DataFlowGraph,
        inst: Inst,
        span: SourceSpan,
        _op: &Ret,
    ) -> anyhow::Result<()> {
        let loc = self.location_from_span(span);
        let args = dfg.inst_args(inst);
        let current_function: FuncOp = self.current_block.operation().unwrap().try_into().unwrap();
        let func_type = current_function.get_type();
        let mut mapped_args = Vec::with_capacity(args.len());
        for (i, mapped_arg) in args.iter().map(|a| self.values[a]).enumerate() {
            let arg_type = mapped_arg.get_type();
            let return_type = func_type.get_result(i).unwrap();

            if arg_type == return_type {
                mapped_args.push(mapped_arg);
            } else {
                let cast = self.cir().build_cast(loc, mapped_arg, return_type);
                mapped_args.push(cast.get_result(0).base());
            }
        }
        self.cir().build_return(loc, mapped_args.as_slice());
        Ok(())
    }

    fn build_ret_imm(
        &self,
        _dfg: &DataFlowGraph,
        _inst: Inst,
        span: SourceSpan,
        op: &RetImm,
    ) -> anyhow::Result<()> {
        let loc = self.location_from_span(span);
        let current_function: FuncOp = self.current_block.operation().unwrap().try_into().unwrap();
        let func_type = current_function.get_type();
        let arg = self.values[&op.arg];
        // Only None is supported as an immediate for this op currently
        assert_eq!(op.imm, Immediate::None);
        let arg_type = arg.get_type();
        let expected_arg_type = func_type.get_result(0).unwrap();
        let expected_imm_type = func_type.get_result(1).unwrap();

        let builder = self.cir();
        let arg = if arg_type == expected_arg_type {
            arg
        } else {
            let cast = builder.build_cast(loc, arg, expected_arg_type);
            cast.get_result(0).base()
        };
        let imm = builder.build_null(loc, expected_imm_type);

        builder.build_return(loc, &[arg, imm.get_result(0).base()]);
        Ok(())
    }

    fn build_br(
        &mut self,
        dfg: &DataFlowGraph,
        inst: Inst,
        span: SourceSpan,
        op: &Br,
    ) -> anyhow::Result<()> {
        let loc = self.location_from_span(span);
        let dest = self.blocks[&op.destination];
        let args = dfg.inst_args(inst);
        let mut mapped_args = Vec::with_capacity(args.len());
        let builder = CirBuilder::new(&self.builder);
        let i1ty = builder.get_i1_type().base();
        for (i, mapped_arg) in args.iter().map(|a| self.values[a]).enumerate() {
            if i == 0 && op.op != Opcode::Br {
                let cond = if mapped_arg.get_type() != i1ty {
                    let cond_cast = builder.build_cast(loc, mapped_arg, builder.get_i1_type());
                    cond_cast.get_result(0).base()
                } else {
                    mapped_arg
                };
                mapped_args.push(cond);
                continue;
            }
            let index = if op.op == Opcode::Br { i } else { i - 1 };
            let expected_ty = dest.get_argument(index).get_type();
            if mapped_arg.get_type() == expected_ty {
                mapped_args.push(mapped_arg.base());
            } else {
                let cast = builder.build_cast(loc, mapped_arg, expected_ty);
                mapped_args.push(cast.get_result(0).base());
            }
        }
        match op.op {
            Opcode::Br => {
                self.cir().build_branch(loc, dest, mapped_args.as_slice());

                Ok(())
            }
            Opcode::BrIf | Opcode::BrUnless => {
                // In syntax_core, control continues after the conditional jump, but in mlir
                // we need to split the original block in two, and jump to either the desired
                // destination block, or the latter half of the original block where we will
                // resume building
                let split_block = {
                    let region = self.current_block.region().unwrap();
                    let block = OwnedBlock::default();
                    let block_ref = block.base();
                    region.insert_after(self.current_block, block);
                    block_ref
                };
                let dest_args = &mapped_args[1..];
                let cond = mapped_args[0];
                if op.op == Opcode::BrIf {
                    builder.build_cond_branch(loc, cond, dest, dest_args, split_block, &[]);
                } else {
                    // For BrUnless, we need to invert the condition
                    builder.build_cond_branch(loc, cond, split_block, &[], dest, dest_args);
                };
                builder.set_insertion_point_to_end(split_block);
                self.current_block = split_block;

                Ok(())
            }
            other => unimplemented!("unrecognized branching op: {}", other),
        }
    }

    fn build_is_type(
        &mut self,
        dfg: &DataFlowGraph,
        inst: Inst,
        span: SourceSpan,
        op: &IsType,
    ) -> anyhow::Result<()> {
        let builder = CirBuilder::new(&self.builder);
        let loc = self.location_from_span(span);
        let input = self.values[&op.arg];
        let ty = translate_ir_type(&self.module, &self.options, &builder, &op.ty);
        let op = builder.build_is_type(loc, input, ty);

        // Map syntax_core results to MLIR results
        let result = dfg.first_result(inst);
        let mlir_result = op.get_result(0);
        self.values.insert(result, mlir_result.base());

        Ok(())
    }

    fn build_primop(
        &mut self,
        dfg: &DataFlowGraph,
        inst: Inst,
        span: SourceSpan,
        op: &PrimOp,
    ) -> anyhow::Result<()> {
        let loc = self.location_from_span(span);
        let args = dfg.inst_args(inst);
        let builder = CirBuilder::new(&self.builder);
        let mlir_op = match op.op {
            Opcode::MatchFail => {
                let class = self.immediate_to_constant(loc, Immediate::Atom(symbols::Error));
                let reason = self.values[&args[0]];
                let trace_op = builder.build_stacktrace(loc);
                let trace = trace_op.get_result(0).base();
                builder.build_raise(loc, class, reason, trace).base()
            }
            Opcode::RecvStart => builder.build_recv_start(loc, self.values[&args[0]]).base(),
            Opcode::RecvNext => builder.build_recv_next(loc, self.values[&args[0]]).base(),
            Opcode::RecvPeek => builder.build_recv_next(loc, self.values[&args[0]]).base(),
            Opcode::RecvPop => builder.build_recv_pop(loc, self.values[&args[0]]).base(),
            Opcode::RecvWait => builder.build_yield(loc).base(),
            Opcode::Raise => {
                let class = self.values[&args[0]];
                let reason = self.values[&args[1]];
                let trace = self.values[&args[2]];
                builder.build_raise(loc, class, reason, trace).base()
            }
            Opcode::BuildStacktrace => builder.build_stacktrace(loc).base(),
            Opcode::ExceptionClass => builder
                .build_exception_class(loc, self.values[&args[0]])
                .base(),
            Opcode::ExceptionReason => builder
                .build_exception_reason(loc, self.values[&args[0]])
                .base(),
            Opcode::ExceptionTrace => builder
                .build_exception_trace(loc, self.values[&args[0]])
                .base(),
            other => unimplemented!("unrecognized primop: {}", other),
        };

        let results = dfg.inst_results(inst);
        for (value, op_result) in results.iter().copied().zip(mlir_op.results()) {
            self.values.insert(value, op_result.base());
        }
        Ok(())
    }

    fn build_primop_imm(
        &mut self,
        dfg: &DataFlowGraph,
        inst: Inst,
        span: SourceSpan,
        op: &PrimOpImm,
    ) -> anyhow::Result<()> {
        let loc = self.location_from_span(span);
        let imm = self.immediate_to_constant(loc, op.imm);
        let args = dfg.inst_args(inst);
        let builder = CirBuilder::new(&self.builder);
        let mlir_op = match op.op {
            Opcode::MatchFail => {
                let class = self.immediate_to_constant(loc, Immediate::Atom(symbols::Error));
                let reason = imm;
                let trace_op = builder.build_stacktrace(loc);
                let trace = trace_op.get_result(0).base();
                builder.build_raise(loc, class, reason, trace).base()
            }
            Opcode::RecvStart => builder.build_recv_start(loc, imm).base(),
            Opcode::Raise => {
                let class = imm;
                let reason = self.values[&args[1]];
                let trace = self.values[&args[2]];
                builder.build_raise(loc, class, reason, trace).base()
            }
            Opcode::BuildStacktrace => builder.build_stacktrace(loc).base(),
            other => unimplemented!("unrecognized primop immediate op: {}", other),
        };

        let results = dfg.inst_results(inst);
        for (value, op_result) in results.iter().copied().zip(mlir_op.results()) {
            self.values.insert(value, op_result.base());
        }
        Ok(())
    }

    fn build_call(
        &mut self,
        dfg: &DataFlowGraph,
        inst: Inst,
        span: SourceSpan,
        op: &Call,
    ) -> anyhow::Result<()> {
        let loc = self.location_from_span(span);
        let args = dfg.inst_args(inst);
        let mapped_args = args.iter().map(|a| self.values[a]).collect::<Vec<_>>();
        let sig = self.find_function(op.callee);
        let name = sig.mfa().to_string();

        let callee = self.get_or_declare_function(name.as_str()).unwrap();
        let mlir_op = self.cir().build_call(loc, callee, mapped_args.as_slice());

        let results = dfg.inst_results(inst);
        for (value, op_result) in results.iter().copied().zip(mlir_op.results()) {
            self.values.insert(value, op_result.base());
        }
        Ok(())
    }

    fn build_call_indirect(
        &mut self,
        dfg: &DataFlowGraph,
        inst: Inst,
        span: SourceSpan,
        op: &CallIndirect,
    ) -> anyhow::Result<()> {
        let loc = self.location_from_span(span);
        let args = dfg.inst_args(inst);
        let mapped_args = args.iter().map(|a| self.values[a]).collect::<Vec<_>>();

        let builder = CirBuilder::new(&self.builder);
        let callee = self.values[&op.callee];
        let mlir_op = builder.build_call_indirect(loc, callee, mapped_args.as_slice());

        let results = dfg.inst_results(inst);
        for (value, op_result) in results.iter().copied().zip(mlir_op.results()) {
            self.values.insert(value, op_result.base());
        }
        Ok(())
    }

    fn build_setelement(
        &mut self,
        dfg: &DataFlowGraph,
        inst: Inst,
        span: SourceSpan,
        op: &SetElement,
    ) -> anyhow::Result<()> {
        let loc = self.location_from_span(span);
        let tuple = self.values[&op.args[0]];
        let index = self.values[&op.args[1]];
        let value = self.values[&op.args[2]];

        let builder = CirBuilder::new(&self.builder);
        let mlir_op = builder.build_set_element(loc, tuple, index, value);
        self.values
            .insert(dfg.first_result(inst), mlir_op.get_result(0).base());

        Ok(())
    }

    fn build_setelement_imm(
        &mut self,
        dfg: &DataFlowGraph,
        inst: Inst,
        span: SourceSpan,
        op: &SetElementImm,
    ) -> anyhow::Result<()> {
        let loc = self.location_from_span(span);
        let tuple = self.values[&op.arg];
        let index = self.immediate_to_constant(loc, op.index);
        let value = self.immediate_to_constant(loc, op.value);

        let builder = CirBuilder::new(&self.builder);
        let mlir_op = builder.build_set_element(loc, tuple, index, value);
        self.values
            .insert(dfg.first_result(inst), mlir_op.get_result(0).base());

        Ok(())
    }

    fn build_setelement_const(
        &mut self,
        dfg: &DataFlowGraph,
        inst: Inst,
        span: SourceSpan,
        op: &SetElementConst,
    ) -> anyhow::Result<()> {
        let loc = self.location_from_span(span);
        let tuple = self.values[&op.arg];
        let index = self.immediate_to_constant(loc, op.index);
        let value = self.const_to_constant(loc, &dfg.constant(op.value));

        let builder = CirBuilder::new(&self.builder);
        let mlir_op = builder.build_set_element(loc, tuple, index, value);
        self.values
            .insert(dfg.first_result(inst), mlir_op.get_result(0).base());

        Ok(())
    }
}

/// Translates a syntax_core type to an equivalent MLIR type
fn translate_ir_type<'a, B: OpBuilder>(
    module: &syntax_core::Module,
    options: &Options,
    builder: &CirBuilder<'a, B>,
    ty: &syntax_core::Type,
) -> TypeBase {
    use liblumen_syntax_core::Type as CoreType;

    debug!("translating syntax_core type {:?} to mlir type", ty);
    match ty {
        CoreType::Invalid | CoreType::NoReturn => builder.get_cir_none_type().base(),
        CoreType::Primitive(ref ty) => translate_primitive_ir_type(builder, ty),
        CoreType::Term(ref ty) => translate_term_ir_type(module, options, builder, ty),
        CoreType::Exception => builder
            .get_cir_ptr_type(builder.get_cir_exception_type())
            .base(),
        CoreType::ExceptionTrace => builder.get_cir_trace_type().base(),
        CoreType::RecvContext => builder.get_cir_recv_context_type().base(),
        CoreType::RecvState => builder.get_i8_type().base(),
    }
}

fn translate_primitive_ir_type<'a, B: OpBuilder>(
    builder: &CirBuilder<'a, B>,
    ty: &syntax_core::PrimitiveType,
) -> TypeBase {
    use liblumen_syntax_core::PrimitiveType;
    match ty {
        PrimitiveType::Void => builder.get_none_type().base(),
        PrimitiveType::I1 => builder.get_i1_type().base(),
        PrimitiveType::I8 => builder.get_i8_type().base(),
        PrimitiveType::I16 => builder.get_i16_type().base(),
        PrimitiveType::I32 => builder.get_i32_type().base(),
        PrimitiveType::I64 => builder.get_i64_type().base(),
        PrimitiveType::Isize => builder.get_index_type().base(),
        PrimitiveType::F64 => builder.get_f64_type().base(),
        PrimitiveType::Ptr(inner) => {
            let inner_ty = translate_primitive_ir_type(builder, &inner);
            builder.get_cir_ptr_type(inner_ty).base()
        }
        PrimitiveType::Struct(fields) => {
            let fields = fields
                .iter()
                .map(|t| translate_primitive_ir_type(builder, t))
                .collect::<Vec<_>>();
            builder.get_struct_type(fields.as_slice()).base()
        }
        PrimitiveType::Array(inner, arity) => {
            let inner_ty = translate_primitive_ir_type(builder, &inner);
            builder.get_array_type(inner_ty, *arity).base()
        }
    }
}

fn translate_term_ir_type<'a, B: OpBuilder>(
    module: &syntax_core::Module,
    options: &Options,
    builder: &CirBuilder<'a, B>,
    ty: &syntax_core::TermType,
) -> TypeBase {
    let use_boxed_floats = !options.target.term_encoding().is_nanboxed();
    match ty {
        TermType::Any => builder.get_cir_term_type().base(),
        TermType::Bool => builder.get_cir_bool_type().base(),
        TermType::Integer => builder.get_cir_integer_type().base(),
        TermType::Float if use_boxed_floats => builder
            .get_cir_box_type(builder.get_cir_float_type())
            .base(),
        TermType::Float => builder.get_cir_float_type().base().base(),
        TermType::Number => builder.get_cir_number_type().base(),
        TermType::Atom => builder.get_cir_atom_type().base(),
        TermType::Bitstring | TermType::Binary => {
            builder.get_cir_box_type(builder.get_cir_bits_type()).base()
        }
        TermType::Nil => builder.get_cir_nil_type().base(),
        TermType::List(_) | TermType::MaybeImproperList => {
            builder.get_cir_box_type(builder.get_cir_cons_type()).base()
        }
        TermType::Tuple(None) => builder.get_cir_box_type(builder.get_tuple_type(&[])).base(),
        TermType::Tuple(Some(ref elems)) => {
            let element_types = elems
                .iter()
                .map(|t| translate_term_ir_type(module, options, builder, t))
                .collect::<Vec<_>>();
            builder
                .get_cir_box_type(builder.get_tuple_type(element_types.as_slice()))
                .base()
        }
        TermType::Map => builder.get_cir_box_type(builder.get_cir_map_type()).base(),
        TermType::Reference => builder.get_cir_reference_type().base(),
        TermType::Port => builder.get_cir_port_type().base(),
        TermType::Pid => builder.get_cir_pid_type().base(),
        TermType::Fun(None) => builder.get_cir_term_type().base(),
        TermType::Fun(Some(func)) => {
            let sig = module.call_signature(*func).clone();
            signature_to_fn_type(module, options, builder, &sig).base()
        }
    }
}

/// Converts a syntax_core Signature to its corresponding MLIR function type
fn signature_to_fn_type<'a, B: OpBuilder>(
    module: &syntax_core::Module,
    options: &Options,
    builder: &CirBuilder<'a, B>,
    sig: &syntax_core::Signature,
) -> FunctionType {
    debug!(
        "translating syntax_core signature {} to mlir function type",
        sig.mfa()
    );
    let param_types = sig
        .params()
        .iter()
        .map(|t| translate_ir_type(module, options, builder, t))
        .collect::<Vec<_>>();
    let result_types = sig
        .results()
        .iter()
        .map(|t| translate_ir_type(module, options, builder, t))
        .collect::<Vec<_>>();
    builder.get_function_type(param_types.as_slice(), result_types.as_slice())
}