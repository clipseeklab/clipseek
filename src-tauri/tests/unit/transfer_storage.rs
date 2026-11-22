use super::*;

#[test]
fn space_budget_keeps_larger_of_one_gib_and_five_percent() {
    assert_eq!(
        required_transfer_space(500, 10 * 1024 * 1024 * 1024),
        500 + 1024 * 1024 * 1024
    );
    assert_eq!(
        required_transfer_space(500, 100 * 1024 * 1024 * 1024),
        500 + 5 * 1024 * 1024 * 1024
    );
    assert_eq!(required_transfer_space(u64::MAX, u64::MAX), u64::MAX);
}
