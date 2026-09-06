use bonaparte_engine::reference::MediaFrames;
use bonaparte_media::cache::DiskPlaybackCache;
use bonaparte_model::{MediaId, Time};

#[test]
fn test_cache_insert_and_get_frame() {
    let cache = DiskPlaybackCache::with_temp_dir(4).expect("cache creation must succeed");

    let media = MediaId(101);
    let time = Time(5000);
    let width = 4;
    let height = 4;

    // Pattern: 16 pixels with distinct values
    let mut rgba = Vec::with_capacity(64);
    for i in 0..16 {
        rgba.extend_from_slice(&[i as u8 * 10, i as u8 * 15, i as u8 * 5, 255]);
    }

    cache
        .insert_frame(media, time, width, height, &rgba)
        .expect("insert must succeed");

    assert!(cache.contains_ram(media, time));
    assert!(cache.contains_disk(media, time));
    assert_eq!(cache.ram_count(), 1);
    assert_eq!(cache.disk_count(), 1);

    // Retrieve via MediaFrames trait
    let view = cache
        .frame_rgba(media, time)
        .expect("frame must be retrievable");
    assert_eq!(view.width, 4);
    assert_eq!(view.height, 4);
    assert_eq!(view.rgba, &rgba[..]);
}

#[test]
fn test_cache_flat_ram_guarantee() {
    // Capped at exactly 4 RAM slots
    let ram_cap = 4;
    let cache = DiskPlaybackCache::with_temp_dir(ram_cap).expect("cache must create");
    let media = MediaId(202);

    let frame_count = 50;
    // Insert 50 frames into cache
    for i in 0..frame_count {
        let time = Time(i as i64 * 4000);
        let pixel_val = (i % 256) as u8;
        let rgba = vec![pixel_val; 16 * 16 * 4];
        cache
            .insert_frame(media, time, 16, 16, &rgba)
            .expect("insert frame");

        // Verification of Flat RAM invariant: RAM slots never exceed capacity
        assert!(
            cache.ram_count() <= ram_cap,
            "RAM count ({}) exceeded capacity ({}) at insert {}",
            cache.ram_count(),
            ram_cap,
            i
        );
    }

    // All 50 frames must exist in Tier 2 disk store
    assert_eq!(cache.disk_count(), frame_count);

    // Now query all 50 frames in reverse order (forces eviction and reloading from disk)
    for i in (0..frame_count).rev() {
        let time = Time(i as i64 * 4000);
        let expected_val = (i % 256) as u8;

        let view = cache
            .frame_rgba(media, time)
            .expect("frame must be available");
        assert_eq!(view.width, 16);
        assert_eq!(view.height, 16);
        assert_eq!(view.rgba[0], expected_val);
        assert_eq!(view.rgba[100], expected_val);

        // Crucial guarantee check: RAM count MUST remain <= ram_cap
        assert!(
            cache.ram_count() <= ram_cap,
            "RAM count ({}) exceeded capacity ({}) during playback seek",
            cache.ram_count(),
            ram_cap
        );
    }
}

#[test]
fn test_cache_clear_cleans_ram_and_disk() {
    let cache = DiskPlaybackCache::with_temp_dir(5).unwrap();
    let media = MediaId(303);

    for i in 0..5 {
        let time = Time(i * 1000);
        cache.insert_frame(media, time, 2, 2, &[255; 16]).unwrap();
    }

    assert_eq!(cache.disk_count(), 5);
    assert_eq!(cache.ram_count(), 5);

    cache.clear().expect("clear must succeed");

    assert_eq!(cache.disk_count(), 0);
    assert_eq!(cache.ram_count(), 0);
    assert!(cache.frame_rgba(media, Time(0)).is_none());
}
