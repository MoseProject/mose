use bush::{Generation, LatestIntent, PendingIntent};

#[test]
fn newest_intent_replaces_unstarted_work() {
    let mut intents = LatestIntent::new();

    assert_eq!(intents.replace(Generation(4), "first"), None);
    assert_eq!(
        intents.replace(Generation(5), "latest"),
        Some(PendingIntent {
            generation: Generation(4),
            value: "first",
        })
    );
    assert_eq!(
        intents.take(),
        Some(PendingIntent {
            generation: Generation(5),
            value: "latest",
        })
    );
    assert!(intents.is_empty());
}

#[test]
fn taking_an_empty_queue_is_a_no_op() {
    let mut intents: LatestIntent<()> = LatestIntent::new();

    assert_eq!(intents.take(), None);
    assert!(intents.is_empty());
}
