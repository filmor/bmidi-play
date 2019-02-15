// extern crate dsp;
extern crate bmidi;
extern crate cpal;
extern crate crossbeam;
extern crate crossbeam_channel;
extern crate dsp;
extern crate synth;
extern crate time_calc;

#[macro_use]
extern crate clap;

use bmidi::{Event, EventType};
use dsp::conv::{ToFrameSliceMut, ToSampleSliceMut};
use dsp::Node;
use std::path::Path;
use time_calc::{Bpm, Ticks};

use std::cmp;

mod sink;
mod source;
mod synth_util;

use sink::Sink as MySink;

fn run() {
    let matches = clap_app!(bmidi_play =>
                  (version: env!("CARGO_PKG_VERSION"))
                  (about: "Simple midi player")
                  (@arg TRK: -t --track +takes_value "The track to play")
                  (@arg CHN: -c --channel +takes_value "The channel to play")
                  (@arg FILENAME: +required "A standard midi file")
                  ).get_matches();

    let channel = value_t!(matches.value_of("CHN"), u8).unwrap_or(0);
    let track = value_t!(matches.value_of("TRK"), usize).unwrap_or(0);
    let filename = matches.value_of("FILENAME").expect("Missing filename");

    println!("Playing track {} of {}", track, filename);

    let mut synth = synth_util::new();

    let endpoint = cpal::default_output_device().expect("Failed to get default endpoint");
    let format = endpoint
        .supported_output_formats()
        .expect("Error while querying supported formats")
        .next()
        .expect("Failed to get endpoint format")
        .with_max_sample_rate();

    let event_loop = cpal::EventLoop::new();

    let stream_id = event_loop
        .build_output_stream(&endpoint, &format)
        .expect("Failed to build output stream");

    let samples_rate = format.sample_rate.0 as f64;
    println!("Format: {:?}", format);

    let (tx, rx) = crossbeam_channel::unbounded();

    let midi_tempo_to_bpm = |tempo: f32| {
        // tempo is µs / beat (mus = 10^-6, min = 6 * 10^1 => min / mus = 6 * 10^7)
        // => bpm = (6 * 10^7) / tempo
        (6e7 / tempo) as Bpm
    };

    // TODO: Implement speed changes
    let bpm = midi_tempo_to_bpm(10e4);

    // How many frames do we still have to write with the current state?
    let mut cursor = 0 as i64;
    let mut next_cursor = 0 as i64;

    let mut current_event = Event {
        delay: 0,
        channel: channel,
        typ: EventType::Meta {
            typ: 0,
            data: vec![],
        },
    };

    println!("Starting to play");

    crossbeam::scope(|scope| {
        source::fill_channel(scope, tx, Path::new(filename), track);

        println!("Starting event loop");

        event_loop.play_stream(stream_id);
        event_loop.run(move |_stream_id, stream_data| {
            match stream_data {
                cpal::StreamData::Output { buffer } => {
                    let len = buffer.len() as i64;

                    let mut buffer = if let cpal::UnknownTypeOutputBuffer::F32(mut inner) = buffer {
                        inner
                    } else {
                        return;
                    };

                    let mut inner_cursor = 0 as i64;
                    let start_cursor = cursor;

                    while next_cursor < start_cursor + len {
                        println!(
                            "\n\nstart: {}\nnext: {}\ncurrent: {}\ninner: {}\nlen: {}",
                            start_cursor, next_cursor, cursor, inner_cursor, len
                        );
                        let frames = cmp::min(next_cursor - cursor, len - inner_cursor);

                        let new_output: &mut [[f32; 2]] = buffer
                            [inner_cursor as usize..(inner_cursor + frames) as usize]
                            .to_sample_slice_mut()
                            .to_frame_slice_mut()
                            .unwrap();

                        synth.audio_requested(new_output, samples_rate);

                        inner_cursor += frames;
                        cursor = next_cursor;

                        match rx.recv() {
                            Some(evt) => {
                                println!("Got next event: {:?}", evt);

                                synth.process_event(&current_event);

                                let skip = Ticks(evt.delay as i64).samples(
                                    bpm,
                                    96, /* ppqn */
                                    samples_rate as f64,
                                );

                                current_event = evt;

                                cursor = next_cursor;
                                next_cursor += skip;
                            }
                            None => {
                                println!("Sender died");
                                return;
                            }
                        }
                    }

                    if inner_cursor < len {
                        let new_output: &mut [[f32; 2]] = &mut buffer
                            [inner_cursor as usize..len as usize]
                            .to_sample_slice_mut()
                            .to_frame_slice_mut()
                            .unwrap();
                        synth.audio_requested(new_output, samples_rate);
                    }

                    cursor += len - inner_cursor;
                    // println!("Processed {} events, advanced cursor by {}", loops, len);
                }
                _ => println!("Unexpected type"),
            }
        });
    });
}

fn main() {
    run(); // .expect("Error running");
}
