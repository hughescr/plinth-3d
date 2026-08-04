use plinth_support::SupportPreparationInput;

fn peek(input: &SupportPreparationInput<'_>) -> usize {
    input.coverage_samples_max_virtual_cells
}

fn main() {}
