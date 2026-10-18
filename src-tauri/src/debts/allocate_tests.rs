use super::fixtures::Rng;
use super::splits::allocate;
use crate::vault::AppError;

#[test]
fn a_hundred_among_three_gives_the_odd_cent_to_the_first() {
    assert_eq!(allocate(10_000, &[1, 1, 1]).unwrap(), [3334, 3333, 3333]);
    assert_eq!(allocate(10_000, &[1, 1, 1, 1]).unwrap(), [2500, 2500, 2500, 2500]);
    assert_eq!(allocate(10_000, &[1, 1]).unwrap(), [5000, 5000]);
}

#[test]
fn the_cents_left_go_to_the_largest_remainders_and_ties_to_the_lower_position() {
    // one cent between two: the first
    assert_eq!(allocate(1, &[1, 1]).unwrap(), [1, 0]);
    assert_eq!(allocate(1, &[1, 1, 1]).unwrap(), [1, 0, 0]);
    assert_eq!(allocate(2, &[1, 1, 1]).unwrap(), [1, 1, 0]);
    // 10 among 1:1:3 = 2, 2, 6 exactly; 11 among them: quotas 2.2, 2.2, 6.6 -> the third has the largest remainder
    assert_eq!(allocate(10, &[1, 1, 3]).unwrap(), [2, 2, 6]);
    assert_eq!(allocate(11, &[1, 1, 3]).unwrap(), [2, 2, 7]);
    // 7 among 2:2:3 -> quotas 2, 2, 3 exactly; 8 -> 2.29, 2.29, 3.43: the third, then the first
    assert_eq!(allocate(8, &[2, 2, 3]).unwrap(), [2, 2, 4]);
    assert_eq!(allocate(100, &[1, 2, 2]).unwrap(), [20, 40, 40]);
    // equal remainders: the lower position wins
    assert_eq!(allocate(5, &[1, 1, 1, 1]).unwrap(), [2, 1, 1, 1]);
}

#[test]
fn a_weight_of_zero_gets_nothing_and_the_whole_can_be_one_persons() {
    assert_eq!(allocate(100, &[0, 1, 1]).unwrap(), [0, 50, 50]);
    assert_eq!(allocate(1, &[0, 1, 1]).unwrap(), [0, 1, 0]);
    assert_eq!(allocate(100, &[0, 0, 7]).unwrap(), [0, 0, 100]);
    assert_eq!(allocate(0, &[1, 2]).unwrap(), [0, 0]);
    assert_eq!(allocate(999_999_999_999, &[1]).unwrap(), [999_999_999_999]);
}

#[test]
fn the_largest_numbers_do_not_overflow() {
    let big = 999_999_999_999u64;
    let total = 999_999_999_999i64;
    let parts = allocate(total, &[big, big, big]).unwrap();
    assert_eq!(parts.iter().sum::<i64>(), total);
    assert!(parts.iter().all(|p| (333_333_333_333..=333_333_333_334).contains(p)));
    let parts = allocate(total, &[big; 31]).unwrap();
    assert_eq!(parts.iter().sum::<i64>(), total);
    let parts = allocate(1, &[big, 1]).unwrap();
    assert_eq!(parts, [1, 0]);
}

#[test]
fn nothing_to_share_with_or_a_negative_total_is_refused() {
    assert_eq!(allocate(100, &[]), Err(AppError::InvalidInput));
    assert_eq!(allocate(100, &[0, 0]), Err(AppError::InvalidInput));
    assert_eq!(allocate(-1, &[1, 1]), Err(AppError::InvalidInput));
}

#[test]
fn for_random_totals_and_weights_the_parts_add_up_and_each_is_within_a_cent_of_its_share() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for round in 0..20_000 {
        let count = rng.between(1, 31) as usize;
        let total = match round % 4 {
            0 => rng.between(0, 20),
            1 => rng.between(0, 100_000),
            2 => rng.between(0, 999_999_999_999),
            _ => rng.between(1, 1_000),
        } as i64;
        let ceiling = [3, 10_000, 999_999_999_999][round % 3];
        let mut weights: Vec<u64> = (0..count).map(|_| if rng.between(0, 4) == 0 { 0 } else { rng.between(1, ceiling) }).collect();
        if weights.iter().all(|w| *w == 0) {
            weights[0] = 1;
        }
        let sum: u128 = weights.iter().map(|w| u128::from(*w)).sum();
        let parts = allocate(total, &weights).unwrap();
        assert_eq!(parts.len(), count);
        assert_eq!(parts.iter().sum::<i64>(), total, "{total} among {weights:?}");
        for (part, weight) in parts.iter().zip(&weights) {
            assert!(*part >= 0);
            if *weight == 0 {
                assert_eq!(*part, 0, "a weight of 0 gets 0: {total} among {weights:?}");
            }
            // the part differs from the exact share total * weight / sum by less than a cent
            let exact = i128::from(total as u64) * i128::from(*weight);
            let scaled = i128::from(*part) * sum as i128;
            assert!((scaled - exact).abs() < sum as i128, "{part} is not within a cent of {total} * {weight} / {sum}");
        }
    }
}

#[test]
fn the_result_depends_on_the_weights_and_not_on_their_size() {
    let mut rng = Rng(0xDEAD_BEEF_1234_5678);
    for _ in 0..2_000 {
        let count = rng.between(1, 12) as usize;
        let weights: Vec<u64> = (0..count).map(|_| rng.between(0, 50)).collect();
        if weights.iter().all(|w| *w == 0) {
            continue;
        }
        let total = rng.between(0, 1_000_000) as i64;
        let k = rng.between(2, 10_000);
        let scaled: Vec<u64> = weights.iter().map(|w| w * k).collect();
        assert_eq!(allocate(total, &weights).unwrap(), allocate(total, &scaled).unwrap(), "{total} among {weights:?} times {k}");
    }
}
