use {
    crate::processor,
    pinocchio::{
        AccountView, Address, ProgramResult, no_allocator, nostd_panic_handler, program_entrypoint,
    },
};

// `entrypoint!` would pull in `default_panic_handler!`, which needs `std`. This
// crate and every one of its dependencies is `no_std`, so the three components
// are declared individually: no allocator is installed at all, because nothing
// in this program allocates.
program_entrypoint!(process_instruction);
no_allocator!();
nostd_panic_handler!();

fn process_instruction(
    program_id: &Address,
    accounts: &mut [AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    processor::process_instruction(program_id, accounts, instruction_data)
}
