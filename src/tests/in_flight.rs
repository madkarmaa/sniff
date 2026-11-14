use super::*;

fn lead<T>(turn: Turn<T>) -> Leader<T> {
    match turn {
        Turn::Lead(leader) => leader,
        Turn::Follow(_) => panic!("expected to lead"),
    }
}

fn follow<T>(turn: Turn<T>) -> Follower<T> {
    match turn {
        Turn::Follow(follower) => follower,
        Turn::Lead(_) => panic!("expected to follow"),
    }
}

#[test]
fn the_first_request_leads_and_later_ones_follow() {
    let lookups = InFlight::<u32>::default();
    let _leader = lead(lookups.join("app", 0));
    let _follower = follow(lookups.join("app", 0));
    let _other = lead(lookups.join("other app", 0));
}

#[test]
fn followers_see_the_outcome_once_the_leader_finishes() {
    let lookups = InFlight::<u32>::default();
    let leader = lead(lookups.join("app", 0));
    let follower = follow(lookups.join("app", 10));
    assert_eq!(follower.progress(20), Progress::Running);

    leader.finish(7);

    assert_eq!(follower.progress(30), Progress::Finished(7));
}

#[test]
fn a_finished_lookup_makes_room_for_a_new_one() {
    let lookups = InFlight::<u32>::default();
    lead(lookups.join("app", 0)).finish(1);
    let _next = lead(lookups.join("app", 0));
}

#[test]
fn followers_learn_when_the_leader_gives_up() {
    let lookups = InFlight::<u32>::default();
    let leader = lead(lookups.join("app", 0));
    let follower = follow(lookups.join("app", 0));

    drop(leader);

    assert_eq!(follower.progress(0), Progress::Abandoned);
    // A follower whose leader gave up can lead the next attempt.
    let _next = lead(lookups.join("app", 0));
}

#[test]
fn a_lookup_that_runs_too_long_is_abandoned_and_replaced() {
    let lookups = InFlight::<u32>::default();
    let stuck = lead(lookups.join("app", 0));
    let follower = follow(lookups.join("app", 0));
    assert_eq!(follower.progress(ABANDON_AFTER_MS - 1), Progress::Running);
    assert_eq!(follower.progress(ABANDON_AFTER_MS), Progress::Abandoned);

    let replacement = lead(lookups.join("app", ABANDON_AFTER_MS));
    // The stuck leader neither removes nor answers for its replacement.
    stuck.finish(1);
    let late = follow(lookups.join("app", ABANDON_AFTER_MS));
    assert_eq!(late.progress(ABANDON_AFTER_MS), Progress::Running);
    replacement.finish(2);
    assert_eq!(late.progress(ABANDON_AFTER_MS), Progress::Finished(2));
}

#[test]
fn clones_share_the_lookups() {
    let lookups = InFlight::<u32>::default();
    let _leader = lead(lookups.join("app", 0));
    let _follower = follow(lookups.clone().join("app", 0));
}
