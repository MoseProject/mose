use super::*;

#[test]
fn producer_replacement_retains_only_the_latest_pending_payload() {
    let (sender, receiver) = channel(1, |pending, value| {
        pending.clear();
        Some(value)
    });
    for value in 0..100 {
        sender.send(value).unwrap();
    }
    assert_eq!(receiver.recv().unwrap(), 99);
    assert_eq!(receiver.try_recv(), Err(TryRecvError::Empty));
}

#[test]
fn a_full_mailbox_preserves_order_and_applies_backpressure() {
    let (sender, receiver) = channel(1, |_, value| Some(value));
    sender.send(1).unwrap();
    let (started, ready) = std::sync::mpsc::channel();
    let (done, completed) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        started.send(()).unwrap();
        sender.send(2).unwrap();
        done.send(()).unwrap();
    });
    ready.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(completed.try_recv(), Err(TryRecvError::Empty));
    assert_eq!(receiver.recv().unwrap(), 1);
    completed.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(receiver.recv().unwrap(), 2);
    worker.join().unwrap();
}

#[test]
fn dropping_the_receiver_releases_a_blocked_sender_with_an_error() {
    let (sender, receiver) = channel(1, |_, value| Some(value));
    sender.send(1).unwrap();
    let worker = std::thread::spawn(move || sender.send(2));
    drop(receiver);
    assert_eq!(worker.join().unwrap().unwrap_err().0, 2);
}
