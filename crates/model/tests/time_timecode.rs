use bonaparte_model::*;

#[test]
#[allow(clippy::op_ref)]
fn test_time_arithmetic_all_operators() {
    let t1 = Time(100);
    let t2 = Time(250);

    // Value + Value
    assert_eq!(t1 + t2, Time(350));
    // Value + Ref
    assert_eq!(t1 + &t2, Time(350));
    // Ref + Value
    assert_eq!(&t1 + t2, Time(350));
    // Ref + Ref
    assert_eq!(&t1 + &t2, Time(350));

    // Subtraction
    assert_eq!(t2 - t1, Time(150));
    assert_eq!(t2 - &t1, Time(150));
    assert_eq!(&t2 - t1, Time(150));
    assert_eq!(&t2 - &t1, Time(150));

    // Multiplication
    assert_eq!(t1 * 4, Time(400));
    assert_eq!(4 * t1, Time(400));

    // Division
    assert_eq!(t2 / 5, Time(50));

    // Negation
    assert_eq!(-t1, Time(-100));
    assert_eq!(-Time(-50), Time(50));

    // Assign operators
    let mut acc = Time(10);
    acc += Time(20);
    assert_eq!(acc, Time(30));
    acc += &Time(15);
    assert_eq!(acc, Time(45));
    acc -= Time(5);
    assert_eq!(acc, Time(40));
    acc -= &Time(10);
    assert_eq!(acc, Time(30));

    // Sum trait
    let list = vec![Time(10), Time(20), Time(30), Time(40)];
    let total1: Time = list.iter().sum();
    assert_eq!(total1, Time(100));
    let total2: Time = list.into_iter().sum();
    assert_eq!(total2, Time(100));
}

#[test]
fn test_framerate_display_and_ticks() {
    let rates = [
        (FrameRate::FPS_24, "24 fps", 5000),
        (FrameRate::FPS_25, "25 fps", 4800),
        (FrameRate::FPS_30, "30 fps", 4000),
        (FrameRate::FPS_60, "60 fps", 2000),
        (FrameRate::NTSC_FILM, "23.976 fps", 5005),
        (FrameRate { num: 30000, den: 1001 }, "29.97 fps", 4004),
        (FrameRate { num: 60000, den: 1001 }, "59.94 fps", 2002),
    ];

    for (rate, display_str, expected_tpf) in rates {
        assert_eq!(rate.to_string(), display_str);
        assert_eq!(rate.ticks_per_frame(), expected_tpf);

        // Test frame conversions
        let frame_num = 120;
        let t = rate.from_frame(frame_num);
        assert_eq!(t, Time(frame_num * expected_tpf));
        assert_eq!(rate.to_frame(t), frame_num);
    }
}

#[test]
fn test_smpte_timecode_conversions() {
    let fps24 = FrameRate::FPS_24;

    // Frame 0
    assert_eq!(Time(0).to_timecode(fps24), "00:00:00:00");
    assert_eq!(Time::from_timecode("00:00:00:00", fps24).unwrap(), Time(0));

    // Frame 1 -> 5000 ticks
    assert_eq!(Time(5000).to_timecode(fps24), "00:00:00:01");
    assert_eq!(Time::from_timecode("00:00:00:01", fps24).unwrap(), Time(5000));

    // Frame 23 (last frame of second 0) -> 115000 ticks
    assert_eq!(Time(115000).to_timecode(fps24), "00:00:00:23");
    assert_eq!(Time::from_timecode("00:00:00:23", fps24).unwrap(), Time(115000));

    // Frame 24 (1 second) -> 120000 ticks
    assert_eq!(Time(120000).to_timecode(fps24), "00:00:01:00");
    assert_eq!(Time::from_timecode("00:00:01:00", fps24).unwrap(), Time(120000));

    // 1 hour, 23 minutes, 45 seconds, 12 frames
    let total_frames = ((60 + 23) * 60 + 45) * 24 + 12;
    let t = Time(total_frames * 5000);
    assert_eq!(t.to_timecode(fps24), "01:23:45:12");
    assert_eq!(Time::from_timecode("01:23:45:12", fps24).unwrap(), t);

    // Support semicolon delimiter (drop-frame style)
    assert_eq!(Time::from_timecode("01;23;45;12", fps24).unwrap(), t);
}

#[test]
fn test_smpte_timecode_errors() {
    let fps24 = FrameRate::FPS_24;

    // Frame out of range (24 >= nominal 24)
    let res1 = Time::from_timecode("00:00:00:24", fps24);
    assert!(matches!(res1, Err(TimecodeError::FrameOutOfRange(24, 24))));

    // Seconds out of range (60 >= 60)
    let res2 = Time::from_timecode("00:00:60:00", fps24);
    assert!(matches!(res2, Err(TimecodeError::SecondOutOfRange(60))));

    // Minutes out of range (60 >= 60)
    let res3 = Time::from_timecode("00:60:00:00", fps24);
    assert!(matches!(res3, Err(TimecodeError::MinuteOutOfRange(60))));

    // Invalid format (too few parts)
    let res4 = Time::from_timecode("00:00:00", fps24);
    assert!(matches!(res4, Err(TimecodeError::InvalidFormat(_))));

    // Invalid non-integer
    let res5 = Time::from_timecode("00:00:xx:00", fps24);
    assert!(matches!(res5, Err(TimecodeError::ParseInt(_))));
}
