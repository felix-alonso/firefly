use ::alloc::sync::Arc;

use crate::erts::*;

mod are_flags_set {
    use super::*;

    #[test]
    fn process_trap_exit_is_not_set_by_default() {
        let process = process();

        assert_eq!(process.are_flags_set(ProcessFlags::TrapExit), false);
    }
}

mod trap_exit {
    use super::*;

    #[test]
    fn process_returns_true_for_the_default_old_value() {
        let process = process();

        assert_eq!(process.trap_exit(true), false);
    }

    #[test]
    fn returns_old_value() {
        let process = process();

        assert_eq!(process.trap_exit(true), false);
        assert_eq!(process.trap_exit(false), true);
    }
}

mod traps_exit {
    use super::*;

    #[test]
    fn process_returns_false_by_default() {
        let process = process();

        assert_eq!(process.traps_exit(), false);
    }

    #[test]
    fn process_returns_true_after_trap_exit_true() {
        let process = process();

        assert_eq!(process.trap_exit(true), false);
        assert_eq!(process.traps_exit(), true);
    }
}

pub(super) fn process() -> Process {
    let init = atom_from_str!("init");
    let initial_module_function_arity = Arc::new(ModuleFunctionArity {
        module: init,
        function: init,
        arity: 0,
    });
    let (heap, heap_size) = alloc::default_heap().unwrap();

    let process = Process::new(
        Priority::Normal,
        None,
        initial_module_function_arity,
        heap,
        heap_size,
    );

    process.schedule_with(scheduler::id::next());

    process
}
