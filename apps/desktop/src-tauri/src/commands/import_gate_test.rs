use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::*;

#[test]
fn no_more_than_the_limit_are_in_at_once() {
    let gate = Arc::new(Gate::new(2));
    let inside = Arc::new(AtomicUsize::new(0));
    let most = Arc::new(AtomicUsize::new(0));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let (gate, inside, most) = (Arc::clone(&gate), Arc::clone(&inside), Arc::clone(&most));
            std::thread::spawn(move || {
                let _permit = gate.enter();
                let now = inside.fetch_add(1, Ordering::SeqCst) + 1;
                most.fetch_max(now, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(15));
                inside.fetch_sub(1, Ordering::SeqCst);
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(most.load(Ordering::SeqCst), 2);
    assert_eq!(gate.running(), 0);
}

#[test]
fn the_waiting_are_let_in_in_the_order_they_came() {
    let gate = Arc::new(Gate::new(1));
    let order = Arc::new(Mutex::new(Vec::new()));
    let holder = gate.enter();
    let mut handles = Vec::new();
    for id in 0..5 {
        let (gate, order) = (Arc::clone(&gate), Arc::clone(&order));
        handles.push(std::thread::spawn(move || {
            let _permit = gate.enter();
            order.lock().unwrap().push(id);
        }));
        // Each is in the queue before the next arrives.
        std::thread::sleep(Duration::from_millis(30));
    }
    drop(holder);
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(*order.lock().unwrap(), vec![0, 1, 2, 3, 4]);
}

#[test]
fn a_limit_of_zero_still_lets_one_through() {
    let gate = Gate::new(0);
    let permit = gate.enter();
    assert_eq!(gate.running(), 1);
    drop(permit);
    assert_eq!(gate.running(), 0);
}

#[test]
fn a_panicking_holder_gives_its_place_back() {
    let gate = Arc::new(Gate::new(1));
    let panicking = Arc::clone(&gate);
    let _ = std::thread::spawn(move || {
        let _permit = panicking.enter();
        panic!("a step failed");
    })
    .join();
    // Not stuck behind the dead holder.
    let _permit = gate.enter();
    assert_eq!(gate.running(), 1);
}

#[test]
fn the_shared_gates_have_a_sensible_size() {
    let analysis = analysis_gate();
    let proxy = proxy_gate();
    let first = proxy.enter();
    assert_eq!(proxy.running(), 1);
    drop(first);
    let _a = analysis.enter();
    assert!(analysis.running() >= 1);
}
