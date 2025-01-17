use firefly_binary::{BinaryEntrySpecifier, BitVec};
use firefly_diagnostics::SourceSpan;
use firefly_intern::{Ident, Symbol};
use firefly_number::{BigInt, Integer};
use firefly_syntax_base::{PrimitiveType, TermType, Type};

use super::*;

pub trait InstBuilderBase<'f>: Sized {
    fn data_flow_graph(&self) -> &DataFlowGraph;
    fn data_flow_graph_mut(&mut self) -> &mut DataFlowGraph;
    fn build(self, data: InstData, ty: Type, span: SourceSpan) -> (Inst, &'f mut DataFlowGraph);
}

macro_rules! binary_op {
    ($name:ident, $op:expr) => {
        paste::paste! {
            fn $name(self, lhs: Value, rhs: Value, span: SourceSpan) -> Value {
                let ty = self.data_flow_graph().value_type(lhs);
                let (inst, dfg) = self.Binary($op, ty, lhs, rhs, span);
                dfg.first_result(inst)
            }
            fn [<$name _imm>](self, lhs: Value, rhs: Immediate, span: SourceSpan) -> Value {
                let ty = self.data_flow_graph().value_type(lhs);
                let (inst, dfg) = self.BinaryImm($op, ty, lhs, rhs, span);
                dfg.first_result(inst)
            }
        }
    };
}
macro_rules! binary_bool_op {
    ($name:ident, $op:expr) => {
        paste::paste! {
            fn $name(self, lhs: Value, rhs: Value, span: SourceSpan) -> Value {
                let (inst, dfg) = self.Binary($op, Type::Term(TermType::Bool), lhs, rhs, span);
                dfg.first_result(inst)
            }
            fn [<$name _imm>](self, lhs: Value, rhs: bool, span: SourceSpan) -> Value {
                let (inst, dfg) = self.BinaryImm($op, Type::Term(TermType::Bool), lhs, Immediate::Term(ImmediateTerm::Bool(rhs)), span);
                dfg.first_result(inst)
            }
        }
    };
}
macro_rules! binary_int_op {
    ($name:ident, $op:expr) => {
        paste::paste! {
            fn $name(self, lhs: Value, rhs: Value, span: SourceSpan) -> Value {
                let (inst, dfg) = self.Binary($op, Type::Term(TermType::Integer), lhs, rhs, span);
                dfg.first_result(inst)
            }
            fn [<$name _imm>](self, lhs: Value, rhs: i64, span: SourceSpan) -> Value {
                let (inst, dfg) = self.BinaryImm($op, Type::Term(TermType::Integer), lhs, Immediate::Term(ImmediateTerm::Integer(rhs)), span);
                dfg.first_result(inst)
            }
        }
    };
}
macro_rules! binary_numeric_op {
    ($name:ident, $op:expr) => {
        paste::paste! {
            fn $name(self, lhs: Value, rhs: Value, span: SourceSpan) -> Value {
                let lty = self.data_flow_graph().value_type(lhs);
                let rty = self.data_flow_graph().value_type(lhs).as_term().unwrap();
                let ty = lty.as_term().unwrap().coerce_to_numeric_with(rty);
                let (inst, dfg) = self.Binary($op, Type::Term(ty), lhs, rhs, span);
                dfg.first_result(inst)
            }
            fn [<$name _imm>](self, lhs: Value, rhs: Immediate, span: SourceSpan) -> Value {
                let lty = self.data_flow_graph().value_type(lhs);
                let rty = rhs.ty().as_term().unwrap();
                assert!(rty.is_numeric(), "invalid immediate value type for arithmetic op");
                let ty = lty.as_term().unwrap().coerce_to_numeric_with(rty);
                let (inst, dfg) = self.BinaryImm($op, Type::Term(ty), lhs, rhs, span);
                dfg.first_result(inst)
            }
        }
    };
}
macro_rules! unary_bool_op {
    ($name:ident, $op:expr) => {
        paste::paste! {
            fn $name(self, arg: Value, span: SourceSpan) -> Value {
                let (inst, dfg) = self.Unary($op, Type::Term(TermType::Bool), arg, span);
                dfg.first_result(inst)
            }
            fn [<$name _imm>](self, imm: bool, span: SourceSpan) -> Value {
                let (inst, dfg) = self.UnaryImm($op, Type::Term(TermType::Bool), Immediate::Term(ImmediateTerm::Bool(imm)), span);
                dfg.first_result(inst)
            }
        }
    };
}
macro_rules! unary_int_op {
    ($name:ident, $op:expr) => {
        paste::paste! {
            fn $name(self, arg: Value, span: SourceSpan) -> Value {
                let (inst, dfg) = self.Unary($op, Type::Term(TermType::Integer), arg, span);
                dfg.first_result(inst)
            }
            fn [<$name _imm>](self, imm: i64, span: SourceSpan) -> Value {
                let (inst, dfg) = self.UnaryImm($op, Type::Term(TermType::Integer), Immediate::Term(ImmediateTerm::Integer(imm)), span);
                dfg.first_result(inst)
            }
        }
    };
}
macro_rules! unary_numeric_op {
    ($name:ident, $op:expr) => {
        paste::paste! {
            fn $name(self, arg: Value, span: SourceSpan) -> Value {
                let ty = self.data_flow_graph().value_type(arg);
                let (inst, dfg) = self.Unary($op, Type::Term(ty.as_term().unwrap().coerce_to_numeric()), arg, span);
                dfg.first_result(inst)
            }
            fn [<$name _imm>](self, imm: Immediate, span: SourceSpan) -> Value {
                let ty = imm.ty().as_term().unwrap();
                assert!(ty.is_numeric(), "invalid immediate value for arithmetic op");
                let (inst, dfg) = self.UnaryImm($op, Type::Term(ty), imm, span);
                dfg.first_result(inst)
            }
        }
    };
}

pub trait InstBuilder<'f>: InstBuilderBase<'f> {
    fn i1(self, i: bool, span: SourceSpan) -> Value {
        let (inst, dfg) = self.UnaryImm(
            Opcode::ImmInt,
            Type::Primitive(PrimitiveType::I1),
            Immediate::I1(i),
            span,
        );
        dfg.first_result(inst)
    }

    fn i8(self, i: i8, span: SourceSpan) -> Value {
        let (inst, dfg) = self.UnaryImm(
            Opcode::ImmInt,
            Type::Primitive(PrimitiveType::I8),
            Immediate::I8(i),
            span,
        );
        dfg.first_result(inst)
    }

    fn i16(self, i: i16, span: SourceSpan) -> Value {
        let (inst, dfg) = self.UnaryImm(
            Opcode::ImmInt,
            Type::Primitive(PrimitiveType::I16),
            Immediate::I16(i),
            span,
        );
        dfg.first_result(inst)
    }

    fn i32(self, i: i32, span: SourceSpan) -> Value {
        let (inst, dfg) = self.UnaryImm(
            Opcode::ImmInt,
            Type::Primitive(PrimitiveType::I32),
            Immediate::I32(i),
            span,
        );
        dfg.first_result(inst)
    }

    fn i64(self, i: i64, span: SourceSpan) -> Value {
        let (inst, dfg) = self.UnaryImm(
            Opcode::ImmInt,
            Type::Primitive(PrimitiveType::I64),
            Immediate::I64(i),
            span,
        );
        dfg.first_result(inst)
    }

    fn isize(self, i: isize, span: SourceSpan) -> Value {
        let (inst, dfg) = self.UnaryImm(
            Opcode::ImmInt,
            Type::Primitive(PrimitiveType::Isize),
            Immediate::Isize(i),
            span,
        );
        dfg.first_result(inst)
    }

    fn f64(self, f: f64, span: SourceSpan) -> Value {
        let (inst, dfg) = self.UnaryImm(
            Opcode::ImmFloat,
            Type::Primitive(PrimitiveType::F64),
            Immediate::F64(f),
            span,
        );
        dfg.first_result(inst)
    }

    fn int(self, i: i64, span: SourceSpan) -> Value {
        match Integer::new(i) {
            Integer::Small(i) => {
                let (inst, dfg) = self.UnaryImm(
                    Opcode::ImmInt,
                    Type::Term(TermType::Integer),
                    Immediate::Term(ImmediateTerm::Integer(i)),
                    span,
                );
                dfg.first_result(inst)
            }
            Integer::Big(i) => self.bigint(i, span),
        }
    }

    fn bigint(mut self, i: BigInt, span: SourceSpan) -> Value {
        let constant = {
            self.data_flow_graph_mut()
                .make_constant(ConstantItem::Integer(Integer::Big(i)))
        };
        let (inst, dfg) = self.UnaryConst(
            Opcode::ConstBigInt,
            Type::Term(TermType::Integer),
            constant,
            span,
        );
        dfg.first_result(inst)
    }

    fn character(self, c: char, span: SourceSpan) -> Value {
        let (inst, dfg) = self.UnaryImm(
            Opcode::ImmInt,
            Type::Term(TermType::Integer),
            Immediate::Term(ImmediateTerm::Integer((c as u32).try_into().unwrap())),
            span,
        );
        dfg.first_result(inst)
    }

    fn float(self, f: f64, span: SourceSpan) -> Value {
        let (inst, dfg) = self.UnaryImm(
            Opcode::ImmFloat,
            Type::Term(TermType::Float),
            Immediate::Term(ImmediateTerm::Float(f)),
            span,
        );
        dfg.first_result(inst)
    }

    fn bool(self, b: bool, span: SourceSpan) -> Value {
        let (inst, dfg) = self.UnaryImm(
            Opcode::ImmBool,
            Type::Term(TermType::Bool),
            Immediate::Term(ImmediateTerm::Bool(b)),
            span,
        );
        dfg.first_result(inst)
    }

    fn atom(self, a: Symbol, span: SourceSpan) -> Value {
        let (inst, dfg) = self.UnaryImm(
            Opcode::ImmAtom,
            Type::Term(TermType::Atom),
            Immediate::Term(ImmediateTerm::Atom(a)),
            span,
        );
        dfg.first_result(inst)
    }

    fn nil(self, span: SourceSpan) -> Value {
        let (inst, dfg) = self.UnaryImm(
            Opcode::ImmNil,
            Type::Term(TermType::Nil),
            Immediate::Term(ImmediateTerm::Nil),
            span,
        );
        dfg.first_result(inst)
    }

    fn none(self, span: SourceSpan) -> Value {
        let (inst, dfg) = self.UnaryImm(
            Opcode::ImmNone,
            Type::Term(TermType::Any),
            Immediate::Term(ImmediateTerm::None),
            span,
        );
        dfg.first_result(inst)
    }

    fn null(self, ty: Type, span: SourceSpan) -> Value {
        let (inst, dfg) = self.UnaryImm(
            Opcode::ImmNull,
            ty,
            Immediate::Term(ImmediateTerm::None),
            span,
        );
        dfg.first_result(inst)
    }

    fn cast(self, arg: Value, ty: Type, span: SourceSpan) -> Value {
        let (inst, dfg) = self.Unary(Opcode::Cast, ty, arg, span);
        dfg.first_result(inst)
    }

    fn binary_from_ident(mut self, id: Ident) -> Value {
        let constant = {
            self.data_flow_graph_mut()
                .make_constant(ConstantItem::InternedStr(id.name))
        };
        let (inst, dfg) = self.UnaryConst(
            Opcode::ConstBinary,
            Type::Term(TermType::Binary),
            constant,
            id.span,
        );
        dfg.first_result(inst)
    }

    fn binary_from_string(mut self, s: String, span: SourceSpan) -> Value {
        let constant = {
            self.data_flow_graph_mut()
                .make_constant(ConstantItem::String(s))
        };
        let (inst, dfg) = self.UnaryConst(
            Opcode::ConstBinary,
            Type::Term(TermType::Binary),
            constant,
            span,
        );
        dfg.first_result(inst)
    }

    fn binary_from_bytes(mut self, bytes: &[u8], span: SourceSpan) -> Value {
        let constant = {
            self.data_flow_graph_mut()
                .make_constant(ConstantItem::Bytes(bytes.to_vec().into()))
        };
        let (inst, dfg) = self.UnaryConst(
            Opcode::ConstBinary,
            Type::Term(TermType::Binary),
            constant,
            span,
        );
        dfg.first_result(inst)
    }

    fn bitstring(mut self, bitvec: BitVec, span: SourceSpan) -> Value {
        let constant = {
            self.data_flow_graph_mut()
                .make_constant(ConstantItem::Bitstring(bitvec))
        };
        let (inst, dfg) = self.UnaryConst(
            Opcode::ConstBinary,
            Type::Term(TermType::Binary),
            constant,
            span,
        );
        dfg.first_result(inst)
    }

    fn is_null(self, arg: Value, span: SourceSpan) -> Value {
        let (inst, dfg) = self.Unary(
            Opcode::IsNull,
            Type::Primitive(PrimitiveType::I1),
            arg,
            span,
        );
        dfg.first_result(inst)
    }

    fn trunc(self, arg: Value, ty: PrimitiveType, span: SourceSpan) -> Value {
        let (inst, dfg) = self.Unary(Opcode::Trunc, Type::Primitive(ty), arg, span);
        dfg.first_result(inst)
    }

    fn zext(self, arg: Value, ty: PrimitiveType, span: SourceSpan) -> Value {
        let (inst, dfg) = self.Unary(Opcode::Zext, Type::Primitive(ty), arg, span);
        dfg.first_result(inst)
    }

    fn zext_imm<I>(self, imm: I, ty: PrimitiveType, span: SourceSpan) -> Value
    where
        I: firefly_number::PrimInt,
        Immediate: From<I>,
    {
        let (inst, dfg) = self.UnaryImm(
            Opcode::Zext,
            Type::Primitive(ty),
            Immediate::from(imm),
            span,
        );
        dfg.first_result(inst)
    }

    fn icmp_eq(self, lhs: Value, rhs: Value, span: SourceSpan) -> Value {
        let (inst, dfg) = self.Binary(
            Opcode::IcmpEq,
            Type::Primitive(PrimitiveType::I1),
            lhs,
            rhs,
            span,
        );
        dfg.first_result(inst)
    }

    fn icmp_eq_imm<I>(self, lhs: Value, rhs: I, span: SourceSpan) -> Value
    where
        I: firefly_number::PrimInt,
        Immediate: From<I>,
    {
        let (inst, dfg) = self.BinaryImm(
            Opcode::IcmpEq,
            Type::Primitive(PrimitiveType::I1),
            lhs,
            Immediate::from(rhs),
            span,
        );
        dfg.first_result(inst)
    }

    fn icmp_neq(self, lhs: Value, rhs: Value, span: SourceSpan) -> Value {
        let (inst, dfg) = self.Binary(
            Opcode::IcmpNeq,
            Type::Primitive(PrimitiveType::I1),
            lhs,
            rhs,
            span,
        );
        dfg.first_result(inst)
    }

    fn icmp_neq_imm<I>(self, lhs: Value, rhs: I, span: SourceSpan) -> Value
    where
        I: firefly_number::PrimInt,
        Immediate: From<I>,
    {
        let (inst, dfg) = self.BinaryImm(
            Opcode::IcmpNeq,
            Type::Primitive(PrimitiveType::I1),
            lhs,
            Immediate::from(rhs),
            span,
        );
        dfg.first_result(inst)
    }

    fn icmp_gt(self, lhs: Value, rhs: Value, span: SourceSpan) -> Value {
        let (inst, dfg) = self.Binary(
            Opcode::IcmpGt,
            Type::Primitive(PrimitiveType::I1),
            lhs,
            rhs,
            span,
        );
        dfg.first_result(inst)
    }

    fn icmp_gt_imm<I>(self, lhs: Value, rhs: I, span: SourceSpan) -> Value
    where
        I: firefly_number::PrimInt,
        Immediate: From<I>,
    {
        let (inst, dfg) = self.BinaryImm(
            Opcode::IcmpGt,
            Type::Primitive(PrimitiveType::I1),
            lhs,
            Immediate::from(rhs),
            span,
        );
        dfg.first_result(inst)
    }

    fn icmp_gte(self, lhs: Value, rhs: Value, span: SourceSpan) -> Value {
        let (inst, dfg) = self.Binary(
            Opcode::IcmpGte,
            Type::Primitive(PrimitiveType::I1),
            lhs,
            rhs,
            span,
        );
        dfg.first_result(inst)
    }

    fn icmp_gte_imm<I>(self, lhs: Value, rhs: I, span: SourceSpan) -> Value
    where
        I: firefly_number::PrimInt,
        Immediate: From<I>,
    {
        let (inst, dfg) = self.BinaryImm(
            Opcode::IcmpGte,
            Type::Primitive(PrimitiveType::I1),
            lhs,
            Immediate::from(rhs),
            span,
        );
        dfg.first_result(inst)
    }

    fn icmp_lt(self, lhs: Value, rhs: Value, span: SourceSpan) -> Value {
        let (inst, dfg) = self.Binary(
            Opcode::IcmpLt,
            Type::Primitive(PrimitiveType::I1),
            lhs,
            rhs,
            span,
        );
        dfg.first_result(inst)
    }

    fn icmp_lt_imm<I>(self, lhs: Value, rhs: I, span: SourceSpan) -> Value
    where
        I: firefly_number::PrimInt,
        Immediate: From<I>,
    {
        let (inst, dfg) = self.BinaryImm(
            Opcode::IcmpLt,
            Type::Primitive(PrimitiveType::I1),
            lhs,
            Immediate::from(rhs),
            span,
        );
        dfg.first_result(inst)
    }

    fn icmp_lte(self, lhs: Value, rhs: Value, span: SourceSpan) -> Value {
        let (inst, dfg) = self.Binary(
            Opcode::IcmpLte,
            Type::Primitive(PrimitiveType::I1),
            lhs,
            rhs,
            span,
        );
        dfg.first_result(inst)
    }

    fn icmp_lte_imm<I>(self, lhs: Value, rhs: I, span: SourceSpan) -> Value
    where
        I: firefly_number::PrimInt,
        Immediate: From<I>,
    {
        let (inst, dfg) = self.BinaryImm(
            Opcode::IcmpLte,
            Type::Primitive(PrimitiveType::I1),
            lhs,
            Immediate::from(rhs),
            span,
        );
        dfg.first_result(inst)
    }

    fn make_fun(mut self, callee: FuncRef, env: &[Value], span: SourceSpan) -> Inst {
        let mut vlist = ValueList::default();
        {
            let pool = &mut self.data_flow_graph_mut().value_lists;
            vlist.extend(env.iter().copied(), pool);
        }
        self.MakeFun(Type::Term(TermType::Fun(None)), callee, vlist, span)
            .0
    }

    fn unpack_env(self, fun: Value, index: usize, span: SourceSpan) -> Value {
        let (inst, dfg) = self.BinaryImm(
            Opcode::UnpackEnv,
            Type::Term(TermType::Any),
            fun,
            Immediate::Isize(index as isize),
            span,
        );
        dfg.first_result(inst)
    }

    fn br(mut self, block: Block, args: &[Value], span: SourceSpan) -> Inst {
        let mut vlist = ValueList::default();
        {
            let pool = &mut self.data_flow_graph_mut().value_lists;
            vlist.extend(args.iter().copied(), pool);
        }
        self.Br(Opcode::Br, Type::Invalid, block, vlist, span).0
    }

    fn br_if(mut self, cond: Value, block: Block, args: &[Value], span: SourceSpan) -> Inst {
        let ty = self.data_flow_graph().value_type(cond);
        let mut vlist = ValueList::default();
        {
            let pool = &mut self.data_flow_graph_mut().value_lists;
            vlist.push(cond, pool);
            vlist.extend(args.iter().copied(), pool);
        }
        self.Br(Opcode::BrIf, ty, block, vlist, span).0
    }

    fn br_unless(mut self, cond: Value, block: Block, args: &[Value], span: SourceSpan) -> Inst {
        let ty = self.data_flow_graph().value_type(cond);
        let mut vlist = ValueList::default();
        {
            let pool = &mut self.data_flow_graph_mut().value_lists;
            vlist.push(cond, pool);
            vlist.extend(args.iter().copied(), pool);
        }
        self.Br(Opcode::BrUnless, ty, block, vlist, span).0
    }

    fn cond_br(
        mut self,
        cond: Value,
        then_dest: Block,
        then_args: &[Value],
        else_dest: Block,
        else_args: &[Value],
        span: SourceSpan,
    ) -> Inst {
        let mut then_vlist = ValueList::default();
        let mut else_vlist = ValueList::default();
        {
            let pool = &mut self.data_flow_graph_mut().value_lists;
            then_vlist.extend(then_args.iter().copied(), pool);
            else_vlist.extend(else_args.iter().copied(), pool);
        }
        self.CondBr(cond, then_dest, then_vlist, else_dest, else_vlist, span)
            .0
    }

    fn switch(self, arg: Value, arms: Vec<(u32, Block)>, default: Block, span: SourceSpan) -> Inst {
        self.Switch(arg, arms, default, span).0
    }

    fn ret(self, is_err: Value, returning: Value, span: SourceSpan) -> Inst {
        self.Ret(is_err, returning, span).0
    }

    fn ret_ok(self, returning: Value, span: SourceSpan) -> Inst {
        self.RetImm(Immediate::I1(false), returning, span).0
    }

    fn ret_err(self, returning: Value, span: SourceSpan) -> Inst {
        self.RetImm(Immediate::I1(true), returning, span).0
    }

    fn call(mut self, callee: FuncRef, args: &[Value], span: SourceSpan) -> Inst {
        let mut vlist = ValueList::default();
        {
            let pool = &mut self.data_flow_graph_mut().value_lists;
            vlist.extend(args.iter().copied(), pool);
        }
        self.Call(Opcode::Call, callee, vlist, span).0
    }

    fn call_indirect(mut self, callee: Value, args: &[Value], span: SourceSpan) -> Inst {
        let mut vlist = ValueList::default();
        {
            let pool = &mut self.data_flow_graph_mut().value_lists;
            vlist.extend(args.iter().copied(), pool);
        }
        self.CallIndirect(Opcode::CallIndirect, callee, vlist, span)
            .0
    }

    fn enter(mut self, callee: FuncRef, args: &[Value], span: SourceSpan) -> Inst {
        let mut vlist = ValueList::default();
        {
            let pool = &mut self.data_flow_graph_mut().value_lists;
            vlist.extend(args.iter().copied(), pool);
        }
        self.Call(Opcode::Enter, callee, vlist, span).0
    }

    fn enter_indirect(mut self, callee: Value, args: &[Value], span: SourceSpan) -> Inst {
        let mut vlist = ValueList::default();
        {
            let pool = &mut self.data_flow_graph_mut().value_lists;
            vlist.extend(args.iter().copied(), pool);
        }
        self.CallIndirect(Opcode::EnterIndirect, callee, vlist, span)
            .0
    }

    fn is_type(self, ty: Type, value: Value, span: SourceSpan) -> Value {
        let (inst, dfg) = self.IsType(ty, value, span);
        dfg.first_result(inst)
    }

    fn cons(self, head: Value, tail: Value, span: SourceSpan) -> Value {
        let (inst, dfg) = self.Binary(Opcode::Cons, Type::Term(TermType::Cons), head, tail, span);
        dfg.first_result(inst)
    }

    fn cons_imm(self, head: Value, tail: Immediate, span: SourceSpan) -> Value {
        let (inst, dfg) =
            self.BinaryImm(Opcode::Cons, Type::Term(TermType::Cons), head, tail, span);
        dfg.first_result(inst)
    }

    fn head(self, list: Value, span: SourceSpan) -> Value {
        let (inst, dfg) = self.Unary(Opcode::Head, Type::Term(TermType::Any), list, span);
        dfg.first_result(inst)
    }

    fn tail(self, list: Value, span: SourceSpan) -> Value {
        let (inst, dfg) = self.Unary(
            Opcode::Tail,
            Type::Term(TermType::MaybeImproperList),
            list,
            span,
        );
        dfg.first_result(inst)
    }

    fn tuple_imm(self, arity: usize, span: SourceSpan) -> Value {
        let (inst, dfg) = self.UnaryImm(
            Opcode::Tuple,
            Type::Term(TermType::Tuple(None)),
            Immediate::Isize(arity as isize),
            span,
        );
        dfg.first_result(inst)
    }

    fn get_element(self, tuple: Value, index: Value, span: SourceSpan) -> Value {
        let (inst, dfg) = self.Binary(
            Opcode::GetElement,
            Type::Term(TermType::Any),
            tuple,
            index,
            span,
        );
        dfg.first_result(inst)
    }

    fn get_element_imm(self, tuple: Value, index: usize, span: SourceSpan) -> Value {
        let (inst, dfg) = self.BinaryImm(
            Opcode::GetElement,
            Type::Term(TermType::Any),
            tuple,
            Immediate::Isize(index as isize),
            span,
        );
        dfg.first_result(inst)
    }

    fn set_element(self, tuple: Value, index: usize, value: Value, span: SourceSpan) -> Value {
        let index = Immediate::Isize(index as isize);
        let (inst, dfg) = self.SetElement(Opcode::SetElement, tuple, index, value, span);
        dfg.first_result(inst)
    }

    fn set_element_mut(self, tuple: Value, index: usize, value: Value, span: SourceSpan) -> Value {
        let index = Immediate::Isize(index as isize);
        let (inst, dfg) = self.SetElement(Opcode::SetElementMut, tuple, index, value, span);
        dfg.first_result(inst)
    }

    fn set_element_imm(
        self,
        tuple: Value,
        index: usize,
        value: Immediate,
        span: SourceSpan,
    ) -> Value {
        let index = Immediate::Isize(index as isize);
        let (inst, dfg) = self.SetElementImm(Opcode::SetElement, tuple, index, value, span);
        dfg.first_result(inst)
    }

    fn set_element_mut_imm(
        self,
        tuple: Value,
        index: usize,
        value: Immediate,
        span: SourceSpan,
    ) -> Value {
        let index = Immediate::Isize(index as isize);
        let (inst, dfg) = self.SetElementImm(Opcode::SetElementMut, tuple, index, value, span);
        dfg.first_result(inst)
    }

    fn recv_start(mut self, timeout: Value, span: SourceSpan) -> Value {
        let mut vlist = ValueList::default();
        {
            let pool = &mut self.data_flow_graph_mut().value_lists;
            vlist.push(timeout, pool);
        }
        let (inst, dfg) = self.PrimOp(Opcode::RecvStart, Type::RecvContext, vlist, span);
        dfg.first_result(inst)
    }

    fn recv_next(mut self, recv_context: Value, span: SourceSpan) -> Value {
        let mut vlist = ValueList::default();
        {
            let pool = &mut self.data_flow_graph_mut().value_lists;
            vlist.push(recv_context, pool);
        }
        let (inst, dfg) = self.PrimOp(Opcode::RecvNext, Type::RecvState, vlist, span);
        dfg.first_result(inst)
    }

    fn recv_peek(mut self, recv_context: Value, span: SourceSpan) -> Value {
        let mut vlist = ValueList::default();
        {
            let pool = &mut self.data_flow_graph_mut().value_lists;
            vlist.push(recv_context, pool);
        }
        let (inst, dfg) = self.PrimOp(Opcode::RecvPeek, Type::Term(TermType::Any), vlist, span);
        dfg.first_result(inst)
    }

    fn recv_pop(mut self, recv_context: Value, span: SourceSpan) -> Inst {
        let mut vlist = ValueList::default();
        {
            let pool = &mut self.data_flow_graph_mut().value_lists;
            vlist.push(recv_context, pool);
        }
        self.PrimOp(Opcode::RecvPop, Type::Invalid, vlist, span).0
    }

    fn recv_wait(mut self, recv_context: Value, span: SourceSpan) -> Inst {
        let mut vlist = ValueList::default();
        {
            let pool = &mut self.data_flow_graph_mut().value_lists;
            vlist.push(recv_context, pool);
        }
        self.PrimOp(Opcode::RecvWait, Type::Invalid, vlist, span).0
    }

    fn bs_test_tail_imm(self, bin: Value, size: usize, span: SourceSpan) -> Value {
        let (inst, dfg) = self.BinaryImm(
            Opcode::BitsTestTail,
            Type::Primitive(PrimitiveType::I1),
            bin,
            Immediate::Isize(size as isize),
            span,
        );
        dfg.first_result(inst)
    }

    fn bs_start_match(mut self, bin: Value, span: SourceSpan) -> Inst {
        let mut vlist = ValueList::default();
        {
            let pool = &mut self.data_flow_graph_mut().value_lists;
            vlist.push(bin, pool);
        }
        self.PrimOp(Opcode::BitsMatchStart, Type::MatchContext, vlist, span)
            .0
    }

    fn bs_match(
        mut self,
        spec: BinaryEntrySpecifier,
        bin: Value,
        size: Option<Value>,
        span: SourceSpan,
    ) -> Inst {
        let mut vlist = ValueList::default();
        {
            let pool = &mut self.data_flow_graph_mut().value_lists;
            vlist.push(bin, pool);
            if let Some(sz) = size {
                vlist.push(sz, pool);
            }
        }
        self.BitsMatch(spec, vlist, span).0
    }

    fn bs_match_skip(
        mut self,
        spec: BinaryEntrySpecifier,
        bin: Value,
        size: Value,
        value: Immediate,
        span: SourceSpan,
    ) -> Inst {
        let mut vlist = ValueList::default();
        {
            let pool = &mut self.data_flow_graph_mut().value_lists;
            vlist.push(bin, pool);
            vlist.push(size, pool);
        }
        self.BitsMatchSkip(spec, vlist, value, span).0
    }

    fn bs_push(
        mut self,
        spec: BinaryEntrySpecifier,
        bin: Value,
        value: Value,
        size: Option<Value>,
        span: SourceSpan,
    ) -> Inst {
        let mut vlist = ValueList::default();
        {
            let pool = &mut self.data_flow_graph_mut().value_lists;
            vlist.push(bin, pool);
            vlist.push(value, pool);
            if let Some(sz) = size {
                vlist.push(sz, pool);
            }
        }
        self.BitsPush(spec, vlist, span).0
    }

    fn raise(mut self, class: Value, error: Value, trace: Value, span: SourceSpan) -> Inst {
        let mut vlist = ValueList::default();
        {
            let pool = &mut self.data_flow_graph_mut().value_lists;
            vlist.push(class, pool);
            vlist.push(error, pool);
            vlist.push(trace, pool);
        }
        self.PrimOp(Opcode::Raise, Type::Invalid, vlist, span).0
    }

    fn exception_class(mut self, exception: Value, span: SourceSpan) -> Value {
        let mut vlist = ValueList::default();
        {
            let pool = &mut self.data_flow_graph_mut().value_lists;
            vlist.push(exception, pool);
        }
        let (inst, dfg) = self.PrimOp(
            Opcode::ExceptionClass,
            Type::Term(TermType::Atom),
            vlist,
            span,
        );
        dfg.first_result(inst)
    }

    fn exception_reason(mut self, exception: Value, span: SourceSpan) -> Value {
        let mut vlist = ValueList::default();
        {
            let pool = &mut self.data_flow_graph_mut().value_lists;
            vlist.push(exception, pool);
        }
        let (inst, dfg) = self.PrimOp(
            Opcode::ExceptionReason,
            Type::Term(TermType::Any),
            vlist,
            span,
        );
        dfg.first_result(inst)
    }

    fn exception_trace(mut self, exception: Value, span: SourceSpan) -> Value {
        let mut vlist = ValueList::default();
        {
            let pool = &mut self.data_flow_graph_mut().value_lists;
            vlist.push(exception, pool);
        }
        let (inst, dfg) = self.PrimOp(
            Opcode::ExceptionTrace,
            Type::Term(TermType::List(None)),
            vlist,
            span,
        );
        dfg.first_result(inst)
    }

    binary_op!(is_tagged_tuple, Opcode::IsTaggedTuple);
    binary_op!(eq, Opcode::Eq);
    binary_op!(eq_exact, Opcode::EqExact);
    binary_op!(neq, Opcode::Neq);
    binary_op!(neq_exact, Opcode::NeqExact);
    binary_op!(gt, Opcode::Gt);
    binary_op!(gte, Opcode::Gte);
    binary_op!(lt, Opcode::Lt);
    binary_op!(lte, Opcode::Lte);
    binary_bool_op!(and, Opcode::And);
    binary_bool_op!(andalso, Opcode::AndAlso);
    binary_bool_op!(or, Opcode::Or);
    binary_bool_op!(orelse, Opcode::OrElse);
    binary_bool_op!(xor, Opcode::Xor);
    binary_int_op!(band, Opcode::Band);
    binary_int_op!(bor, Opcode::Bor);
    binary_int_op!(bxor, Opcode::Bxor);
    binary_int_op!(div, Opcode::Div);
    binary_int_op!(rem, Opcode::Rem);
    binary_int_op!(bsl, Opcode::Bsl);
    binary_int_op!(bsr, Opcode::Bsr);
    binary_numeric_op!(add, Opcode::Add);
    binary_numeric_op!(sub, Opcode::Sub);
    binary_numeric_op!(mul, Opcode::Mul);
    binary_numeric_op!(fdiv, Opcode::Fdiv);
    unary_numeric_op!(neg, Opcode::Neg);
    unary_bool_op!(not, Opcode::Not);
    unary_int_op!(bnot, Opcode::Bnot);

    fn list_concat(self, lhs: Value, rhs: Value, span: SourceSpan) -> Value {
        let (inst, dfg) = self.Binary(
            Opcode::ListConcat,
            Type::Term(TermType::List(None)),
            lhs,
            rhs,
            span,
        );
        dfg.first_result(inst)
    }

    fn list_subtract(self, lhs: Value, rhs: Value, span: SourceSpan) -> Value {
        let (inst, dfg) = self.Binary(
            Opcode::ListSubtract,
            Type::Term(TermType::List(None)),
            lhs,
            rhs,
            span,
        );
        dfg.first_result(inst)
    }

    #[allow(non_snake_case)]
    fn MakeFun(
        self,
        ty: Type,
        callee: FuncRef,
        env: ValueList,
        span: SourceSpan,
    ) -> (Inst, &'f mut DataFlowGraph) {
        let data = InstData::MakeFun(MakeFun { callee, env });
        self.build(data, ty, span)
    }

    #[allow(non_snake_case)]
    fn CondBr(
        self,
        cond: Value,
        then_dest: Block,
        then_args: ValueList,
        else_dest: Block,
        else_args: ValueList,
        span: SourceSpan,
    ) -> (Inst, &'f mut DataFlowGraph) {
        let data = InstData::CondBr(CondBr {
            cond,
            then_dest: (then_dest, then_args),
            else_dest: (else_dest, else_args),
        });
        self.build(data, Type::Invalid, span)
    }

    #[allow(non_snake_case)]
    fn Br(
        self,
        op: Opcode,
        ty: Type,
        destination: Block,
        args: ValueList,
        span: SourceSpan,
    ) -> (Inst, &'f mut DataFlowGraph) {
        let data = InstData::Br(Br {
            op,
            destination,
            args,
        });
        self.build(data, ty, span)
    }

    #[allow(non_snake_case)]
    fn Switch(
        self,
        arg: Value,
        arms: Vec<(u32, Block)>,
        default: Block,
        span: SourceSpan,
    ) -> (Inst, &'f mut DataFlowGraph) {
        let data = InstData::Switch(Switch {
            op: Opcode::Switch,
            arg,
            arms,
            default,
        });
        self.build(data, Type::Invalid, span)
    }

    #[allow(non_snake_case)]
    fn Ret(
        self,
        is_err: Value,
        returning: Value,
        span: SourceSpan,
    ) -> (Inst, &'f mut DataFlowGraph) {
        let data = InstData::Ret(Ret {
            op: Opcode::Ret,
            args: [is_err, returning],
        });
        self.build(data, Type::Invalid, span)
    }

    #[allow(non_snake_case)]
    fn RetImm(
        self,
        is_err: Immediate,
        returning: Value,
        span: SourceSpan,
    ) -> (Inst, &'f mut DataFlowGraph) {
        let data = InstData::RetImm(RetImm {
            op: Opcode::Ret,
            imm: is_err,
            arg: returning,
        });
        self.build(data, Type::Invalid, span)
    }

    #[allow(non_snake_case)]
    fn Call(
        self,
        op: Opcode,
        callee: FuncRef,
        args: ValueList,
        span: SourceSpan,
    ) -> (Inst, &'f mut DataFlowGraph) {
        let data = InstData::Call(Call { op, callee, args });
        self.build(data, Type::Invalid, span)
    }

    #[allow(non_snake_case)]
    fn CallIndirect(
        self,
        op: Opcode,
        callee: Value,
        args: ValueList,
        span: SourceSpan,
    ) -> (Inst, &'f mut DataFlowGraph) {
        let data = InstData::CallIndirect(CallIndirect { op, callee, args });
        self.build(data, Type::Invalid, span)
    }

    #[allow(non_snake_case)]
    fn IsType(self, ty: Type, arg: Value, span: SourceSpan) -> (Inst, &'f mut DataFlowGraph) {
        let data = InstData::IsType(IsType { arg, ty });
        self.build(data, Type::Term(TermType::Bool), span)
    }

    #[allow(non_snake_case)]
    fn Binary(
        self,
        op: Opcode,
        ty: Type,
        lhs: Value,
        rhs: Value,
        span: SourceSpan,
    ) -> (Inst, &'f mut DataFlowGraph) {
        let data = InstData::BinaryOp(BinaryOp {
            op,
            args: [lhs, rhs],
        });
        self.build(data, ty, span)
    }

    #[allow(non_snake_case)]
    fn BinaryImm(
        self,
        op: Opcode,
        ty: Type,
        arg: Value,
        imm: Immediate,
        span: SourceSpan,
    ) -> (Inst, &'f mut DataFlowGraph) {
        let data = InstData::BinaryOpImm(BinaryOpImm { op, arg, imm });
        self.build(data, ty, span)
    }

    #[allow(non_snake_case)]
    fn Unary(
        self,
        op: Opcode,
        ty: Type,
        arg: Value,
        span: SourceSpan,
    ) -> (Inst, &'f mut DataFlowGraph) {
        let data = InstData::UnaryOp(UnaryOp { op, arg });
        self.build(data, ty, span)
    }

    #[allow(non_snake_case)]
    fn UnaryImm(
        self,
        op: Opcode,
        ty: Type,
        imm: Immediate,
        span: SourceSpan,
    ) -> (Inst, &'f mut DataFlowGraph) {
        let data = InstData::UnaryOpImm(UnaryOpImm { op, imm });
        self.build(data, ty, span)
    }

    #[allow(non_snake_case)]
    fn UnaryConst(
        self,
        op: Opcode,
        ty: Type,
        imm: Constant,
        span: SourceSpan,
    ) -> (Inst, &'f mut DataFlowGraph) {
        let data = InstData::UnaryOpConst(UnaryOpConst { op, imm });
        self.build(data, ty, span)
    }

    #[allow(non_snake_case)]
    fn PrimOp(
        self,
        op: Opcode,
        ty: Type,
        args: ValueList,
        span: SourceSpan,
    ) -> (Inst, &'f mut DataFlowGraph) {
        let data = InstData::PrimOp(PrimOp { op, args });
        self.build(data, ty, span)
    }

    #[allow(non_snake_case)]
    fn PrimOpImm(
        self,
        op: Opcode,
        ty: Type,
        imm: Immediate,
        args: ValueList,
        span: SourceSpan,
    ) -> (Inst, &'f mut DataFlowGraph) {
        let data = InstData::PrimOpImm(PrimOpImm { op, imm, args });
        self.build(data, ty, span)
    }

    #[allow(non_snake_case)]
    fn BitsMatch(
        self,
        spec: BinaryEntrySpecifier,
        args: ValueList,
        span: SourceSpan,
    ) -> (Inst, &'f mut DataFlowGraph) {
        let ty = match spec {
            BinaryEntrySpecifier::Integer { .. } => Type::Term(TermType::Integer),
            BinaryEntrySpecifier::Float { .. } => Type::Term(TermType::Float),
            BinaryEntrySpecifier::Utf8
            | BinaryEntrySpecifier::Utf16 { .. }
            | BinaryEntrySpecifier::Utf32 { .. } => Type::Term(TermType::Integer),
            BinaryEntrySpecifier::Binary { unit: 8, .. } => Type::Term(TermType::Binary),
            BinaryEntrySpecifier::Binary { .. } => Type::Term(TermType::Bitstring),
        };
        let data = InstData::BitsMatch(BitsMatch { spec, args });
        self.build(data, ty, span)
    }

    #[allow(non_snake_case)]
    fn BitsMatchSkip(
        self,
        spec: BinaryEntrySpecifier,
        args: ValueList,
        value: Immediate,
        span: SourceSpan,
    ) -> (Inst, &'f mut DataFlowGraph) {
        let data = InstData::BitsMatchSkip(BitsMatchSkip { spec, args, value });
        self.build(data, Type::Invalid, span)
    }

    #[allow(non_snake_case)]
    fn BitsPush(
        self,
        spec: BinaryEntrySpecifier,
        args: ValueList,
        span: SourceSpan,
    ) -> (Inst, &'f mut DataFlowGraph) {
        let data = InstData::BitsPush(BitsPush { spec, args });
        self.build(data, Type::Term(TermType::Any), span)
    }

    #[allow(non_snake_case)]
    fn SetElement(
        self,
        op: Opcode,
        tuple: Value,
        index: Immediate,
        value: Value,
        span: SourceSpan,
    ) -> (Inst, &'f mut DataFlowGraph) {
        let data = InstData::SetElement(SetElement {
            op,
            index,
            args: [tuple, value],
        });
        self.build(data, Type::Term(TermType::Tuple(None)), span)
    }

    #[allow(non_snake_case)]
    fn SetElementImm(
        self,
        op: Opcode,
        tuple: Value,
        index: Immediate,
        value: Immediate,
        span: SourceSpan,
    ) -> (Inst, &'f mut DataFlowGraph) {
        let data = InstData::SetElementImm(SetElementImm {
            op,
            arg: tuple,
            index,
            value,
        });
        self.build(data, Type::Term(TermType::Tuple(None)), span)
    }
}

impl<'f, T: InstBuilderBase<'f>> InstBuilder<'f> for T {}
