use super::device::RawFileHandler;
use openeb_core::hal::device::device::Device;
use openeb_core::hal::errors::StreamError;
use openeb_core::hal::facilities::{
    EventDecoderFacilityHandle, EventsStreamDecoderFacilityHandle, EventsStreamFacilityHandle,
    FacilityError, FacilityType,
};
use std::path::PathBuf;

#[test]
fn test_read_and_decode_raw_evt3() -> Result<(), Box<dyn std::error::Error>> {
    let mut file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    file_path.push("tests");
    file_path.push("sample.raw");

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

    let cd_receiver = event_decoder.subscribe_to_event_buffer();

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
