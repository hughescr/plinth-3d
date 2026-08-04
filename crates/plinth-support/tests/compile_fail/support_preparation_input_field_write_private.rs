use plinth_support::SupportPreparationInput;

fn tamper(input: &mut SupportPreparationInput<'_>) {
    input.max_grid_bytes = 0;
}

fn main() {}
