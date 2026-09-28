use bush::{FrameMailbox, Generation, PixelFormat, PublishedFrame, VideoFrame};

fn frame(generation: u64, timestamp_secs: f64) -> PublishedFrame {
    PublishedFrame {
        generation: Generation(generation),
        frame: VideoFrame::new(PixelFormat::Rgba8, 1, 1, 4, timestamp_secs, vec![0; 4]).unwrap(),
    }
}

#[test]
fn matching_generation_is_presented_once() {
    let mut mailbox = FrameMailbox::new();
    assert!(mailbox.publish(frame(4, 1.0)).is_none());

    assert_eq!(
        mailbox
            .take_for(Generation(4))
            .map(|frame| frame.timestamp_secs()),
        Some(1.0)
    );
    assert!(mailbox.is_empty());
}

#[test]
fn stale_frame_is_discarded_without_presentation() {
    let mut mailbox = FrameMailbox::new();
    mailbox.publish(frame(4, 1.0));

    assert!(mailbox.take_for(Generation(5)).is_none());
    assert!(mailbox.is_empty());
}

#[test]
fn future_frame_waits_for_its_generation() {
    let mut mailbox = FrameMailbox::new();
    mailbox.publish(frame(6, 1.0));

    assert!(mailbox.take_for(Generation(5)).is_none());
    assert_eq!(
        mailbox
            .take_for(Generation(6))
            .map(|frame| frame.timestamp_secs()),
        Some(1.0)
    );
}

#[test]
fn new_completion_replaces_unconsumed_result() {
    let mut mailbox = FrameMailbox::new();
    mailbox.publish(frame(4, 1.0));

    assert_eq!(
        mailbox.publish(frame(5, 2.0)).unwrap().generation,
        Generation(4)
    );
    assert_eq!(
        mailbox
            .take_for(Generation(5))
            .map(|frame| frame.timestamp_secs()),
        Some(2.0)
    );
}
