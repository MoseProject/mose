use bush::{PlaybackState, Transport, TransportError};

#[test]
fn paused_transport_does_not_advance() {
    let mut transport = Transport::new(2.0, 1.5).unwrap();

    transport.advance(3.0).unwrap();

    assert_eq!(transport.position_secs(), 2.0);
    assert_eq!(transport.state(), PlaybackState::Paused);
}

#[test]
fn playing_transport_advances_at_its_rate() {
    let mut transport = Transport::new(1.0, 1.5).unwrap();
    transport.play();
    transport.advance(2.0).unwrap();

    assert_eq!(transport.position_secs(), 4.0);
    assert_eq!(transport.state(), PlaybackState::Playing);
}

#[test]
fn seek_and_rate_change_keep_transport_valid() {
    let mut transport = Transport::new(0.0, 1.0).unwrap();
    transport.seek(8.0).unwrap();
    transport.set_rate(0.5).unwrap();
    transport.play();
    transport.advance(4.0).unwrap();
    transport.pause();

    assert_eq!(transport.position_secs(), 10.0);
    assert_eq!(transport.rate(), 0.5);
    assert_eq!(transport.state(), PlaybackState::Paused);
}

#[test]
fn transport_rejects_non_finite_negative_and_zero_rate_input() {
    assert_eq!(
        Transport::new(-1.0, 1.0),
        Err(TransportError::InvalidPosition(-1.0))
    );
    assert_eq!(
        Transport::new(0.0, 0.0),
        Err(TransportError::InvalidRate(0.0))
    );

    let mut transport = Transport::new(0.0, 1.0).unwrap();
    assert!(matches!(
        transport.advance(f64::NAN),
        Err(TransportError::InvalidElapsed(value)) if value.is_nan()
    ));
}
