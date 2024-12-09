use crate::deadline::DeadlineInfo;


fn default_deadline() -> DeadlineInfo<u64> {
    let block_number = 112;
    let period_start = 100;
    let deadline_index = 0;
    let period_deadlines = 3;
    let proving_period = 60;
    let challenge_window = 20;
    let challenge_lookback = 20;
    let cutoff = 5;

    DeadlineInfo::<u64>::new(
        block_number,
        period_start,
        deadline_index,
        period_deadlines,
        proving_period,
        challenge_window,
        challenge_lookback,
        cutoff
    ).unwrap()
}

#[test]
fn calculates_next_deadline_when_its_open() {
    let deadline_info = default_deadline();
    assert_eq!(deadline_info.is_open(), true);

    let next = deadline_info.next().unwrap();

    assert_eq!(next.open_at, 160);
    assert_eq!(next.close_at, 180);
}

#[test]
fn calculates_next_deadline_when_its_elapsed() {
    let mut deadline_info = default_deadline();
    deadline_info.block_number = 121;
    assert_eq!(deadline_info.is_open(), false);
    assert_eq!(deadline_info.has_elapsed(), true);

    let next = deadline_info.next().unwrap();

    assert_eq!(next.open_at, 160);
    assert_eq!(next.close_at, 180);
}

#[test]
fn calculates_next_deadline_when_its_2_proving_periods_behind() {
    let mut deadline_info = default_deadline();
    deadline_info.block_number = 162;
    assert_eq!(deadline_info.is_open(), false);
    assert_eq!(deadline_info.has_elapsed(), true);

    let next = deadline_info.next().unwrap();

    assert_eq!(next.open_at, 220);
    assert_eq!(next.close_at, 240);
}