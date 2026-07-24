use super::device::RawFileHandler;
use super::RawFileReader;
use openeb_core::hal::device::device::Device;
use openeb_core::hal::errors::StreamError;
use openeb_core::hal::facilities::{
    EventDecoderFacilityHandle, EventsStreamDecoderFacilityHandle, EventsStreamFacilityHandle,
    FacilityError, FacilityType,
};
use std::path::PathBuf;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn sample_raw_path() -> PathBuf {
    let mut file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    file_path.push("tests");
    file_path.push("sample.raw");
    file_path
}

#[test]
fn test_read_and_decode_raw_evt3() -> TestResult {
    let file_path = sample_raw_path();

    let device = RawFileHandler::<131_072>::new_from_path(
        file_path
            .into_os_string()
            .to_str()
            .expect("A cargo manifest dir must be specified."),
    )
    .expect(
        "Failed to initialize device from path. Check if the file exists and the header is valid.",
    );

    let stream_handle: EventsStreamFacilityHandle = device
        .get_facility(FacilityType::EventsStreamFacility)
        .expect("EventsStreamFacility was not registered")
        .try_into()
        .unwrap();

    let mut stream = stream_handle.write().unwrap();

    let decoder_handle: EventsStreamDecoderFacilityHandle = device
        .get_facility(FacilityType::EventsStreamDecoderFacility)
        .expect("EventsStreamDecoderFacility was not registered")
        .try_into()
        .unwrap();

    let event_decoder_handle: EventDecoderFacilityHandle = device
        .get_facility(FacilityType::EventDecoderFacility)
        .expect("EventDecoderFacility was not registered")
        .try_into()
        .unwrap();

    let mut decoder = decoder_handle.write().unwrap();
    let mut event_decoder = event_decoder_handle.write().unwrap();

    let cd_receiver = event_decoder.subscribe_to_cd_events();

    stream.start().expect("Failed to start stream");

    let mut total_bytes_read = 0;
    let mut chunks_processed = 0;

    loop {
        match stream.wait_next_buffer() {
            Ok((buffer, size)) => {
                chunks_processed += 1;
                total_bytes_read += size;
                decoder.decode(buffer)?;

                while let Ok(event_batch) = cd_receiver.try_recv() {
                    for event in event_batch.iter() {
                        dbg!("Event: {}", event);
                    }
                }
            }
            Err(FacilityError::Stream(StreamError::EndOfFile)) => {
                break;
            }
            Err(e) => {
                panic!("Unexpected stream error: {:?}", e);
            }
        }
    }

    stream.stop().expect("Failed to stop stream");

    assert!(
        total_bytes_read > 0,
        "Stream completed but zero bytes were read."
    );
    assert!(chunks_processed > 0, "No chunks were processed.");

    println!(
        "Successfully parsed {} bytes across {} chunks.",
        total_bytes_read, chunks_processed
    );

    Ok(())
}

#[test]
fn test_raw_file_reader_requires_index_for_seek() -> TestResult {
    let file_path = sample_raw_path();
    let mut reader = RawFileReader::<131_072>::try_from_file(
        file_path
            .to_str()
            .expect("A cargo manifest dir must be specified."),
        false,
    )?;

    let err = reader.seek(0).expect_err("Seek without an index must fail");
    assert!(
        matches!(err, crate::types::DeviceFileError::UnsupportedBehavior(_)),
        "Expected UnsupportedBehavior, got {err:?}"
    );

    Ok(())
}

#[test]
fn test_raw_file_reader_can_seek_when_indexed() -> TestResult {
    let file_path = sample_raw_path();
    let mut reader = RawFileReader::<131_072>::try_from_file(
        file_path
            .to_str()
            .expect("A cargo manifest dir must be specified."),
        true,
    )?;

    reader.seek(0)?;

    Ok(())
}

#[test]
fn test_raw_file_reader_can_open_after_default_construction() -> TestResult {
    let file_path = sample_raw_path();
    let mut reader = RawFileReader::<131_072>::new();

    reader.try_open(
        file_path
            .to_str()
            .expect("A cargo manifest dir must be specified."),
        true,
    )?;
    reader.seek(0)?;

    Ok(())
}

#[test]
fn test_raw_file_reader_load_batch_publishes_cd_events() -> TestResult {
    let file_path = sample_raw_path();
    let mut reader = RawFileReader::<131_072>::try_from_file(
        file_path
            .to_str()
            .expect("A cargo manifest dir must be specified."),
        false,
    )?;
    let receiver = reader.cd_receiver()?;

    reader.load_batch()?;

    let mut event_count = 0;
    while let Ok(batch) = receiver.try_recv() {
        event_count += batch.len();
    }

    assert!(event_count > 0, "Expected at least one decoded CD event");

    Ok(())
}

#[test]
fn test_sync_event_window_iterator_loads_batches_on_demand() -> TestResult {
    let file_path = sample_raw_path();
    let mut reader = RawFileReader::<4_096>::try_from_file(
        file_path
            .to_str()
            .expect("A cargo manifest dir must be specified."),
        false,
    )?;
    let mut windows = reader.as_windows()?.into_sync();
    let batch = windows.next_batch()?;

    assert_eq!(batch.len(), 4_096);
    Ok(())
}

#[test]
fn test_sync_event_window_iterator_slices_delta_windows() -> TestResult {
    let file_path = sample_raw_path();
    let mut reader = RawFileReader::<4_096>::try_from_file(
        file_path
            .to_str()
            .expect("A cargo manifest dir must be specified."),
        false,
    )?;
    let mut windows = reader.as_windows()?.into_sync();
    let events = windows.next_delta(10_000)?;

    assert!(!events.is_empty());
    assert!(events.iter().all(|event| event.t < 10_000));
    Ok(())
}

#[test]
fn test_sync_event_window_iterator_returns_partial_window_at_eof() -> TestResult {
    let file_path = sample_raw_path();
    let mut reader = RawFileReader::<4_096>::try_from_file(
        file_path
            .to_str()
            .expect("A cargo manifest dir must be specified."),
        false,
    )?;
    let mut windows = reader.as_windows()?.into_sync();
    let events = windows.next_delta(1_000_000_000)?;

    assert!(!events.is_empty());
    Ok(())
}
