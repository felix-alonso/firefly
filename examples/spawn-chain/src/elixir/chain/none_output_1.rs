use std::sync::Arc;

use liblumen_alloc::erts::exception::system::Alloc;
use liblumen_alloc::erts::process::{code, ProcessControlBlock};
use liblumen_alloc::erts::term::{atom_unchecked, Atom, Term};
use liblumen_alloc::erts::ModuleFunctionArity;

pub fn closure(process: &ProcessControlBlock) -> Result<Term, Alloc> {
    process.closure_with_env_from_slice(module_function_arity(), code, process.pid_term(), &[])
}

/// defp none_output(_text) do
///   :ok
/// end
fn code(arc_process: &Arc<ProcessControlBlock>) -> code::Result {
    arc_process.reduce();

    let _text = arc_process.stack_pop().unwrap();

    ProcessControlBlock::return_from_call(arc_process, atom_unchecked("ok"))?;

    ProcessControlBlock::call_code(arc_process)
}

fn function() -> Atom {
    Atom::try_from_str("none_output").unwrap()
}

fn module_function_arity() -> Arc<ModuleFunctionArity> {
    Arc::new(ModuleFunctionArity {
        module: super::module(),
        function: function(),
        arity: 1,
    })
}
