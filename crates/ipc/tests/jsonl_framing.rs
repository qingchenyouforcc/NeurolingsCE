use ipc::{FrameError, MAX_FRAME_BYTES, decode, encode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, PartialEq, Serialize)]
struct Ping {
    command: String,
}

#[test]
fn encodes_object_with_one_line_terminator() {
    let frame = encode(&Ping {
        command: "ping".to_owned(),
    })
    .expect("object should encode");

    assert_eq!(frame, b"{\"command\":\"ping\"}\n");
    assert!(frame.len() <= MAX_FRAME_BYTES);
}

#[test]
fn decodes_lf_and_crlf_terminated_objects() {
    let expected = Ping {
        command: "ping".to_owned(),
    };

    assert_eq!(
        decode::<Ping>(b"{\"command\":\"ping\"}\n").unwrap(),
        expected
    );
    assert_eq!(
        decode::<Ping>(b"{\"command\":\"ping\"}\r\n").unwrap(),
        expected
    );
}

#[test]
fn rejects_empty_frames() {
    assert!(matches!(decode::<Ping>(b""), Err(FrameError::EmptyFrame)));
    assert!(matches!(decode::<Ping>(b"\n"), Err(FrameError::EmptyFrame)));
    assert!(matches!(
        decode::<Ping>(b"\r\n"),
        Err(FrameError::EmptyFrame)
    ));
}

#[test]
fn rejects_multiple_lines() {
    assert!(matches!(
        decode::<Ping>(b"{\"command\":\"ping\"}\n{\"command\":\"pong\"}\n"),
        Err(FrameError::MultipleLines)
    ));
}

#[test]
fn rejects_frame_larger_than_one_mib() {
    let oversized = vec![b'a'; MAX_FRAME_BYTES + 1];
    assert!(matches!(
        decode::<Ping>(&oversized),
        Err(FrameError::FrameTooLarge { .. })
    ));
}

#[test]
fn rejects_invalid_json() {
    assert!(matches!(
        decode::<Ping>(b"{\"command\":}\n"),
        Err(FrameError::InvalidJson(_))
    ));
}

#[test]
fn rejects_non_object_json() {
    assert!(matches!(
        decode::<Ping>(b"[]\n"),
        Err(FrameError::NotObject)
    ));
}

#[test]
fn rejects_non_object_values_when_encoding() {
    assert!(matches!(encode(&["ping"]), Err(FrameError::NotObject)));
}

#[test]
fn rejects_embedded_newline_and_bare_carriage_return() {
    assert!(matches!(
        decode::<Ping>(b"{\"command\":\"ping\"}\n\n"),
        Err(FrameError::MultipleLines)
    ));
    assert!(matches!(
        decode::<Ping>(b"{\"command\":\"ping\"}\r"),
        Err(FrameError::MultipleLines)
    ));
}
