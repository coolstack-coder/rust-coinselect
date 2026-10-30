use crate::{
    types::{CoinSelectionOpt, OutputGroup, SelectionError, SelectionOutput, WasteMetric},
    utils::{calculate_fee, calculate_waste},
};

/// Performs coin selection using the First-In-First-Out (FIFO) algorithm.
///
/// Returns `NoSolutionFound` if no solution is found.
pub fn select_coin_fifo(
    inputs: &[OutputGroup],
    options: &CoinSelectionOpt,
) -> Result<SelectionOutput, SelectionError> {
    let mut accumulated_value: u64 = 0;
    let mut accumulated_weight: u64 = 0;
    let mut selected_inputs: Vec<usize> = Vec::new();
    let mut estimated_fees: u64 = 0;

    // Sorting the inputs vector based on creation_sequence
    let mut sorted_inputs: Vec<_> = inputs
        .iter()
        .enumerate()
        .filter(|(_, og)| og.creation_sequence.is_some())
        .collect();

    sorted_inputs.sort_by(|a, b| a.1.creation_sequence.cmp(&b.1.creation_sequence));

    let inputs_without_sequence: Vec<_> = inputs
        .iter()
        .enumerate()
        .filter(|(_, og)| og.creation_sequence.is_none())
        .collect();

    sorted_inputs.extend(inputs_without_sequence);

    let target = options.target_value + options.min_change_value;

    // Fees are recomputed after every input is added, so the final check below
    // always accounts for the weight of all the selected inputs.
    for (index, input) in sorted_inputs {
        accumulated_value += input.value;
        accumulated_weight += input.weight;
        estimated_fees = calculate_fee(accumulated_weight, options.target_feerate)?;
        selected_inputs.push(index);

        if accumulated_value >= (target + estimated_fees.max(options.min_absolute_fee)) {
            break;
        }
    }

    if accumulated_value < (target + estimated_fees.max(options.min_absolute_fee)) {
        Err(SelectionError::InsufficientFunds)
    } else {
        let waste: u64 = calculate_waste(
            options,
            accumulated_value,
            accumulated_weight,
            estimated_fees,
        );
        Ok(SelectionOutput {
            selected_inputs,
            waste: WasteMetric(waste),
        })
    }
}

#[cfg(test)]
mod test {

    use crate::{
        algorithms::fifo::select_coin_fifo,
        types::{CoinSelectionOpt, ExcessStrategy, OutputGroup, SelectionError},
    };

    fn setup_basic_output_groups() -> Vec<OutputGroup> {
        vec![
            OutputGroup {
                value: 1000,
                weight: 100,
                input_count: 1,
                creation_sequence: None,
            },
            OutputGroup {
                value: 2000,
                weight: 200,
                input_count: 1,
                creation_sequence: None,
            },
            OutputGroup {
                value: 3000,
                weight: 300,
                input_count: 1,
                creation_sequence: None,
            },
        ]
    }
    fn setup_output_groups_withsequence() -> Vec<OutputGroup> {
        vec![
            OutputGroup {
                value: 1000,
                weight: 100,
                input_count: 1,
                creation_sequence: Some(1),
            },
            OutputGroup {
                value: 2000,
                weight: 200,
                input_count: 1,
                creation_sequence: Some(5000),
            },
            OutputGroup {
                value: 3000,
                weight: 300,
                input_count: 1,
                creation_sequence: Some(1001),
            },
            OutputGroup {
                value: 1500,
                weight: 150,
                input_count: 1,
                creation_sequence: None,
            },
        ]
    }

    fn setup_options(target_value: u64) -> CoinSelectionOpt {
        CoinSelectionOpt {
            target_value,
            target_feerate: 0.4, // Simplified feerate
            long_term_feerate: Some(0.4),
            min_absolute_fee: 0,
            base_weight: 10,
            change_weight: 50,
            change_cost: 10,
            avg_input_weight: 20,
            avg_output_weight: 10,
            min_change_value: 500,
            excess_strategy: ExcessStrategy::ToChange,
        }
    }

    fn test_successful_selection() {
        let mut inputs = setup_basic_output_groups();
        let mut options = setup_options(2500);
        let mut result = select_coin_fifo(&inputs, &options);
        assert!(result.is_ok());
        let mut selection_output = result.unwrap();
        assert!(!selection_output.selected_inputs.is_empty());

        inputs = setup_output_groups_withsequence();
        options = setup_options(500);
        result = select_coin_fifo(&inputs, &options);
        assert!(result.is_ok());
        selection_output = result.unwrap();
        assert!(!selection_output.selected_inputs.is_empty());
    }

    fn test_insufficient_funds() {
        let inputs = setup_basic_output_groups();
        let options = setup_options(7000); // Set a target value higher than the sum of all inputs
        let result = select_coin_fifo(&inputs, &options);
        assert!(matches!(result, Err(SelectionError::InsufficientFunds)));
    }

    fn test_fee_of_last_input_is_accounted() {
        // The second input pushes the accumulated weight to 1100 WU, which costs
        // 440 sats at 0.4 sat/WU. 3100 < 2500 + 500 + 440, so the selection must
        // fail instead of succeeding with the fee of the first input only (40 sats).
        let inputs = [
            OutputGroup {
                value: 3000,
                weight: 100,
                input_count: 1,
                creation_sequence: Some(0),
            },
            OutputGroup {
                value: 100,
                weight: 1000,
                input_count: 1,
                creation_sequence: Some(1),
            },
        ];
        let options = setup_options(2500);
        let result = select_coin_fifo(&inputs, &options);
        assert!(matches!(result, Err(SelectionError::InsufficientFunds)));
    }

    fn test_invalid_feerate() {
        let inputs = setup_basic_output_groups();

        let mut options = setup_options(2500);
        options.target_feerate = 0.0;
        let result = select_coin_fifo(&inputs, &options);
        assert!(matches!(result, Err(SelectionError::NonPositiveFeeRate)));

        options.target_feerate = 1001.0;
        let result = select_coin_fifo(&inputs, &options);
        assert!(matches!(result, Err(SelectionError::AbnormallyHighFeeRate)));
    }

    #[test]
    fn test_fifo() {
        test_successful_selection();
        test_insufficient_funds();
        test_fee_of_last_input_is_accounted();
        test_invalid_feerate();
    }
}
