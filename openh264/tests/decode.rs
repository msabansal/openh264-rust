#![cfg(feature = "decoder")]

use std::io::Read;

use openh264::decoder::{Decoder, DecoderConfig};
use openh264::{nal_units, Error};

#[test]
fn can_get_decoder() -> Result<(), Error> {
    let config = DecoderConfig::default();
    let _decoder = Decoder::with_config(config)?;

    Ok(())
}

#[test]
fn can_access_raw_api() -> Result<(), Error> {
    let config = DecoderConfig::default();
    let mut decoder = Decoder::with_config(config)?;

    unsafe {
        let _ = decoder.raw_api();
    };

    Ok(())
}

#[test]
#[rustfmt::skip]
fn can_decode_single() -> Result<(), Error> {
    let sources = [
        &include_bytes!("data/single_1920x1080_cabac.h264")[..],
        &include_bytes!("data/single_512x512_cabac.h264")[..],
        &include_bytes!("data/single_512x512_cavlc.h264")[..],
    ];

    for (_, src) in sources.iter().enumerate() {
        let config = DecoderConfig::default().debug(false);
        let mut decoder = Decoder::with_config(config)?;

        let yuv = decoder.decode(src)?;

        let dim = yuv.dimension_rgb();
        let rgb_len = dim.0 * dim.1 * 3;
        let mut rgb = vec![0; rgb_len];

        yuv.write_rgb8(&mut rgb)?;
    }

    Ok(())
}

#[test]
fn can_decode_multi_to_end() -> Result<(), Error> {
    let src = &include_bytes!("data/multi_512x512.h264")[..];

    let config = DecoderConfig::default().debug(false);
    let mut decoder = Decoder::with_config(config)?;

    decoder.decode(src)?;

    Ok(())
}

#[test]
fn can_decode_multi_by_step() -> Result<(), Error> {
    let src = &include_bytes!("data/multi_512x512.h264")[..];

    let config = DecoderConfig::default();
    let mut decoder = Decoder::with_config(config)?;

    let mut last_was_ok = false;

    for packet in nal_units(src) {
        last_was_ok = decoder.decode(packet).is_ok()
    }

    assert!(last_was_ok);

    Ok(())
}

#[test]
fn decodes_file_requiring_flush_frame() -> Result<(), Error> {
    let src = &include_bytes!("data/multi_1024x768.raw")[..];

    let config = DecoderConfig::default();
    let mut decoder = Decoder::with_config(config)?;

    for packet in read_frame(src) {
        decoder.decode(packet.as_slice())?;
    }

    Ok(())
}

#[test]
fn fails_on_truncated() -> Result<(), Error> {
    let src = &include_bytes!("data/multi_512x512_truncated.h264")[..];

    let config = DecoderConfig::default().debug(false);
    let mut decoder = Decoder::with_config(config)?;

    assert!(decoder.decode(src).is_err());

    Ok(())
}

#[test]
#[cfg(feature = "encoder")]
fn what_goes_around_comes_around() -> Result<(), Error> {
    use openh264::encoder::{Encoder, EncoderConfig};
    use openh264::formats::RBGYUVConverter;

    let src = &include_bytes!("data/lenna_128x128.rgb")[..];

    let config = EncoderConfig::new(128, 128);
    let mut encoder = Encoder::with_config(config)?;
    let mut converter = RBGYUVConverter::new(128, 128);

    converter.convert(src);

    let stream = encoder.encode(&converter)?;

    let src = stream.to_vec();
    let config = DecoderConfig::default();
    let mut decoder = Decoder::with_config(config)?;
    decoder.decode(&src)?;

    Ok(())
}

// The packets in the file are written frame by frame
// the first 4 bytes are frame length in little endian
// followed by actual frame data
pub fn read_frame<T>(mut stream: T) -> impl Iterator<Item = Vec<u8>> 
where
    T: Read,
{
    std::iter::from_fn(move || {
        let mut data = [0u8; 4];
        let result = stream.read_exact(data.as_mut_slice());
        if result.is_err() {
            return None;
        }
        
        let len = u32::from_le_bytes(data) as usize;
        let mut data = vec![0u8; len];

        let result = stream.read_exact(data.as_mut_slice());
        if result.is_err() {
            return None
        } else {
            Some(data)
        }
    })
}