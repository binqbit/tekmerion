use solana_program_test::ProgramTest;

/// The runner selects the built contract through BPF_OUT_DIR. ProgramTest
/// supplies its standard SPL programs through its locked Cargo dependency.
pub fn program_test() -> ProgramTest {
    ProgramTest::new("tekmerion", tekmerion::ID, None)
}
